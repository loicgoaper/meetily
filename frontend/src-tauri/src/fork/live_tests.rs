// fork/live_tests.rs
//
// Real calls to OpenRouter with the code paths used by the app. Ignored by default
// (they cost a few cents). Run with:
//   OPENROUTER_API_KEY=... MEETILY_TEST_ASSETS=<dir> cargo test --lib fork::live_tests -- --ignored --nocapture
// The assets dir must contain: chunk_fr.wav (10-30 s of French speech),
// meeting.mp3 (a multi-speaker French recording) and slide.jpg (an image with text).

use super::diarization::{align, naming};
use super::meeting_context::{MeetingContext, Participant};
use super::openrouter_chat::{self, ChatMessage, ContentPart};
use super::openrouter_stt::{self, SttOptions};
use std::path::PathBuf;
use std::time::{Duration, Instant};

fn key() -> String {
    std::env::var("OPENROUTER_API_KEY").expect("OPENROUTER_API_KEY not set")
}

fn asset(name: &str) -> PathBuf {
    PathBuf::from(std::env::var("MEETILY_TEST_ASSETS").expect("MEETILY_TEST_ASSETS not set")).join(name)
}

#[tokio::test]
#[ignore]
async fn live_stt_chunk_scribe_and_gpt4o_with_vocabulary() {
    let wav = std::fs::read(asset("chunk_fr.wav")).unwrap();
    let client = openrouter_stt::build_client(Duration::from_secs(45));
    for (model, vocabulary) in [
        ("elevenlabs/scribe-v2", vec![]),
        ("openai/gpt-4o-transcribe", vec!["Thibaudier".to_string(), "Escarbagnas".to_string()]),
    ] {
        let opts = SttOptions { model: model.into(), language: Some("fr".into()), vocabulary, ..Default::default() };
        let started = Instant::now();
        let r = openrouter_stt::transcribe_with_retry(&client, &key(), &wav, "wav", &opts).await.unwrap();
        println!("[{}] {:.1}s cost={:?}\n  {}", model, started.elapsed().as_secs_f64(), r.usage.as_ref().and_then(|u| u.cost), r.text);
        assert!(r.text.split_whitespace().count() > 10, "{} returned too little text", model);
    }
}

#[tokio::test]
#[ignore]
async fn live_diarization_multi_part_and_naming() {
    // Same pipeline as diarize_meeting, minus the database
    let audio = asset("meeting.mp3");
    let duration = super::diarization::audio_duration(&audio).unwrap();
    let plan = super::diarization::plan_parts(duration);
    println!("duration {:.0}s -> {} part(s): {:?}", duration, plan.len(), plan);
    let dir = tempfile::tempdir().unwrap();
    let client = openrouter_stt::build_client(Duration::from_secs(240));
    let opts = SttOptions { model: "elevenlabs/scribe-v2".into(), language: Some("fr".into()), diarize: true, verbose: true, ..Default::default() };

    let mut handles = Vec::new();
    for (index, &(offset, len)) in plan.iter().enumerate() {
        let (path, ext) = super::diarization::encode_part_for_tests(&audio, dir.path(), index, offset, len).unwrap();
        let (client, opts, key) = (client.clone(), opts.clone(), key());
        handles.push(tokio::spawn(async move {
            let bytes = std::fs::read(&path).unwrap();
            let started = Instant::now();
            let r = openrouter_stt::transcribe_with_retry(&client, &key, &bytes, ext, &opts).await.unwrap();
            println!("part {} ({:.0}s, {} KB) answered in {:.1}s, cost {:?}", index, len, bytes.len() / 1024, started.elapsed().as_secs_f64(), r.usage.as_ref().and_then(|u| u.cost));
            (offset, len, r)
        }));
    }
    let mut parts = Vec::new();
    for h in handles {
        let (offset, len, r) = h.await.unwrap();
        parts.push(align::PartWords { offset, duration: len, words: super::diarization::words_for_tests(r) });
    }
    let words = align::merge_parts(parts);
    let segments = align::build_segments(&words);
    let stats = align::speaker_stats(&segments);
    println!("{} words, {} segments, speakers: {:?}", words.len(), segments.len(), stats);
    for s in segments.iter().take(8) {
        println!("  [{:>6.1}] {}: {}", s.start, s.speaker, s.text.chars().take(90).collect::<String>());
    }
    assert!(stats.len() >= 2, "expected several speakers");
    assert!(segments.windows(2).all(|w| w[0].start <= w[1].start), "segments must be in time order");

    // Naming from a participant list (characters of the play)
    let context = MeetingContext {
        context_text: Some("Lecture de La Comtesse d'Escarbagnas de Molière.".into()),
        participants: ["La Comtesse", "Julie", "Le Vicomte", "Andrée"]
            .iter()
            .map(|n| Participant { name: n.to_string(), ..Default::default() })
            .collect(),
        ..Default::default()
    };
    let messages = [
        ChatMessage::system(naming::SYSTEM_PROMPT),
        ChatMessage::user(vec![ContentPart::text(naming::build_user_prompt(&segments, &context, None))]),
    ];
    let outcome = openrouter_chat::chat(&key(), "google/gemini-3.1-flash-lite", &messages, true, 1500, false).await.unwrap();
    let json = openrouter_chat::extract_json(&outcome.text).expect("naming answer is JSON");
    let keys = segments.iter().map(|s| s.speaker.clone()).collect();
    let names = naming::parse_answer(&json, &context, &keys);
    println!("naming cost {:?}: {:?}", outcome.cost_usd, names);
}

#[tokio::test]
#[ignore]
async fn live_vision_reads_a_slide() {
    let bytes = std::fs::read(asset("slide.jpg")).unwrap();
    let messages = [
        ChatMessage::system(super::vision::SYSTEM_PROMPT),
        ChatMessage::user(vec![
            ContentPart::text("1 screenshots, in chronological order. Answer for each id."),
            ContentPart::text("id 1 — shown at [12:30]"),
            ContentPart::jpeg(&bytes),
        ]),
    ];
    let outcome = openrouter_chat::chat(&key(), "google/gemini-3.1-flash-lite", &messages, true, 1000, false).await.unwrap();
    println!("vision cost {:?}: {}", outcome.cost_usd, outcome.text);
    let json = openrouter_chat::extract_json(&outcome.text).expect("vision answer is JSON");
    let content = json["frames"][0]["content"].as_str().unwrap_or_default().to_lowercase();
    assert!(content.contains("siris") || content.contains("budget"), "slide text not read: {}", content);
}

#[tokio::test]
#[ignore]
async fn live_summary_native_french_with_context() {
    use crate::summary::llm_client::LLMProvider;
    let template = crate::summary::templates::get_template("fr_suivi_projet_client").unwrap();
    let transcript = "[00:05] Loïc : Bonjour Marie, on fait le point sur la migration SQL.\n\
        [00:12] Marie : Oui. Côté client, on a validé le plan de bascule pour le 15 novembre.\n\
        [00:30] Loïc : Parfait. Il me manque encore les accès au serveur IIS de recette.\n\
        [00:41] Marie : Je vous les envoie d'ici vendredi. Par contre le budget formation est réduit de moitié.\n\
        [01:02] Loïc : D'accord, je prépare une proposition révisée pour mardi.";
    let custom = "<meeting_context>\nParticipants:\n- Loïc (RISS) [the person who recorded the meeting]\n- Marie Tremblay (DSI, Acme)\nBackground:\nMigration SQL/IIS pour Acme\n</meeting_context>\n\n<screen_content>\n[00:20] Plan de bascule — Bascule le 15 novembre, recette le 8\n</screen_content>";
    let client = reqwest::Client::new();
    let generated = crate::summary::processor::generate_meeting_summary(
        &client,
        &LLMProvider::OpenRouter,
        "google/gemini-3.1-flash-lite",
        &key(),
        transcript,
        custom,
        "fr_suivi_projet_client",
        &template,
        100_000,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        Some("fr"),
        Some("fr"),
        None,
    )
    .await
    .unwrap();
    println!("{}", generated.final_markdown);
    let md = generated.final_markdown.to_lowercase();
    assert!(md.contains("marie"), "participant name missing");
    assert!(md.contains("15 novembre"), "date from transcript/screen missing");
    assert!(md.contains("avancement") || md.contains("décisions"), "French template sections missing");
}
