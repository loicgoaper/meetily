// fork/db_tests.rs
//
// Integration tests of the fork's SQL against a real SQLite database migrated with
// every upstream + fork migration.

use super::meeting_context::{MeetingContext, MeetingContextRepository, Participant};
use super::preferences::{PreferencesRepository, STT_LIVE_MODEL, UI_LANGUAGE};
use super::speakers::{MeetingSpeaker, SpeakersRepository};
use super::tags::TagsRepository;
use sqlx::SqlitePool;
use std::collections::HashMap;

async fn migrated_pool() -> SqlitePool {
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
    let mut migrator = sqlx::migrate!("./migrations");
    migrator.set_ignore_missing(true);
    migrator.run(&pool).await.unwrap();
    sqlx::query("INSERT INTO meetings (id, title, created_at, updated_at) VALUES ('m1', 'Réunion', '2026-10-20', '2026-10-20')")
        .execute(&pool)
        .await
        .unwrap();
    pool
}

fn context(company: Option<&str>) -> MeetingContext {
    MeetingContext {
        context_text: Some("Suivi projet".into()),
        meeting_type: Some("visio".into()),
        participants: vec![
            Participant { name: "Loïc".into(), is_self: true, ..Default::default() },
            Participant { name: "Marie".into(), role: Some("DSI".into()), company: company.map(String::from), ..Default::default() },
        ],
        ..Default::default()
    }
}

#[tokio::test]
async fn context_roundtrip_and_contacts_upsert() {
    let pool = migrated_pool().await;
    MeetingContextRepository::save(&pool, "m1", &context(Some("Acme"))).await.unwrap();
    // Saving again replaces participants and upserts contacts (expression index conflict)
    MeetingContextRepository::save(&pool, "m1", &context(Some("Acme"))).await.unwrap();

    let ctx = MeetingContextRepository::get(&pool, "m1").await.unwrap();
    assert_eq!(ctx.context_text.as_deref(), Some("Suivi projet"));
    assert_eq!(ctx.participants.len(), 2);
    assert!(ctx.participants[0].is_self);
    assert_eq!(ctx.participants[1].company.as_deref(), Some("Acme"));

    let contacts = MeetingContextRepository::search_contacts(&pool, "mar").await.unwrap();
    assert_eq!(contacts.len(), 1, "same name + company must not duplicate");
    let by_company = MeetingContextRepository::search_contacts(&pool, "acme").await.unwrap();
    assert_eq!(by_company.len(), 1);

    // Same name without company is another contact
    MeetingContextRepository::save(&pool, "m1", &context(None)).await.unwrap();
    assert_eq!(MeetingContextRepository::search_contacts(&pool, "marie").await.unwrap().len(), 2);
}

#[tokio::test]
async fn missing_context_is_empty() {
    let pool = migrated_pool().await;
    assert!(MeetingContextRepository::get(&pool, "m1").await.unwrap().is_empty());
}

#[tokio::test]
async fn preferences_defaults_and_overrides() {
    let pool = migrated_pool().await;
    assert_eq!(PreferencesRepository::get(&pool, STT_LIVE_MODEL).await, "elevenlabs/scribe-v2");
    let mut values = HashMap::new();
    values.insert(UI_LANGUAGE.to_string(), "en".to_string());
    PreferencesRepository::set_many(&pool, &values).await.unwrap();
    values.insert(UI_LANGUAGE.to_string(), "fr".to_string());
    PreferencesRepository::set_many(&pool, &values).await.unwrap();
    let all = PreferencesRepository::get_all(&pool).await.unwrap();
    assert_eq!(all[UI_LANGUAGE], "fr");
    assert_eq!(all[STT_LIVE_MODEL], "elevenlabs/scribe-v2");
}

#[tokio::test]
async fn speakers_replace_and_manual_rename() {
    let pool = migrated_pool().await;
    let auto = vec![
        MeetingSpeaker { speaker_key: "speaker_0".into(), display_name: Some("Loïc".into()), participant_id: None, source: "mic-anchor".into(), confidence: None },
        MeetingSpeaker { speaker_key: "speaker_1".into(), display_name: None, participant_id: None, source: "auto".into(), confidence: None },
    ];
    SpeakersRepository::replace_all(&pool, "m1", &auto).await.unwrap();
    SpeakersRepository::rename(&pool, "m1", "speaker_1", Some("  Marie ")).await.unwrap();
    SpeakersRepository::rename(&pool, "m1", "system", Some("Client")).await.unwrap();
    let list = SpeakersRepository::list(&pool, "m1").await.unwrap();
    assert_eq!(list.len(), 3);
    let marie = list.iter().find(|s| s.speaker_key == "speaker_1").unwrap();
    assert_eq!(marie.display_name.as_deref(), Some("Marie"));
    assert_eq!(marie.source, "manual");
}

#[tokio::test]
async fn transcripts_keep_speaker_column() {
    let pool = migrated_pool().await;
    sqlx::query(
        "INSERT INTO transcripts (id, meeting_id, transcript, timestamp, audio_start_time, audio_end_time, duration, speaker)
         VALUES ('t1', 'm1', 'Bonjour', 'now', 0.0, 1.0, 1.0, 'mic')",
    )
    .execute(&pool)
    .await
    .unwrap();
    let rows = sqlx::query_as::<_, crate::database::models::Transcript>("SELECT * FROM transcripts WHERE meeting_id = 'm1'")
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(rows[0].speaker.as_deref(), Some("mic"));
}

#[tokio::test]
async fn tags_unique_names_and_cascades() {
    let pool = migrated_pool().await;
    sqlx::query("INSERT INTO tags (id, name, color, created_at) VALUES ('a', 'Acme', 'blue', 'now'), ('b', 'SIRIS', 'red', 'now')")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO meeting_tags (meeting_id, tag_id) VALUES ('m1', 'a'), ('m1', 'b')")
        .execute(&pool)
        .await
        .unwrap();
    let names: Vec<String> = TagsRepository::for_meeting(&pool, "m1").await.unwrap().into_iter().map(|t| t.name).collect();
    assert_eq!(names, vec!["Acme", "SIRIS"]);
    assert_eq!(TagsRepository::meetings_with_tag(&pool, "b").await.unwrap(), vec!["m1"]);

    // Names are unique whatever the case
    assert!(sqlx::query("INSERT INTO tags (id, name, created_at) VALUES ('c', 'acme', 'now')").execute(&pool).await.is_err());

    // Deleting a tag, then the meeting, removes the assignments
    sqlx::query("DELETE FROM tags WHERE id = 'b'").execute(&pool).await.unwrap();
    assert_eq!(TagsRepository::all_assignments(&pool).await.unwrap()["m1"], vec!["a"]);
    sqlx::query("DELETE FROM meetings WHERE id = 'm1'").execute(&pool).await.unwrap();
    assert!(TagsRepository::all_assignments(&pool).await.unwrap().is_empty());
}

#[tokio::test]
async fn merge_speakers_moves_every_line() {
    let pool = migrated_pool().await;
    sqlx::query(
        "INSERT INTO transcripts (id, meeting_id, transcript, timestamp, audio_start_time, audio_end_time, duration, speaker) VALUES
         ('t1', 'm1', 'Bonjour', 'now', 0.0, 1.0, 1.0, 'speaker_0'),
         ('t2', 'm1', 'Salut', 'now', 1.0, 2.0, 1.0, 'speaker_3'),
         ('t3', 'm1', 'Ça va ?', 'now', 2.0, 3.0, 1.0, 'speaker_3')",
    )
    .execute(&pool)
    .await
    .unwrap();
    let speakers = vec![
        MeetingSpeaker { speaker_key: "speaker_0".into(), display_name: Some("Olivier".into()), participant_id: None, source: "auto".into(), confidence: None },
        MeetingSpeaker { speaker_key: "speaker_3".into(), display_name: None, participant_id: None, source: "auto".into(), confidence: None },
    ];
    SpeakersRepository::replace_all(&pool, "m1", &speakers).await.unwrap();

    assert_eq!(SpeakersRepository::merge(&pool, "m1", "speaker_3", "speaker_0").await.unwrap(), 2);
    let keys: Vec<String> = SpeakersRepository::list(&pool, "m1").await.unwrap().into_iter().map(|s| s.speaker_key).collect();
    assert_eq!(keys, vec!["speaker_0"]);
    let left: Vec<(String,)> = sqlx::query_as("SELECT DISTINCT speaker FROM transcripts WHERE meeting_id = 'm1'").fetch_all(&pool).await.unwrap();
    assert_eq!(left, vec![("speaker_0".to_string(),)]);
}

#[tokio::test]
async fn tag_suggestions_accept_and_dismiss() {
    use super::tag_suggest;
    let pool = migrated_pool().await;
    sqlx::query("INSERT INTO tags (id, name, color, created_at) VALUES ('a', 'Acme', 'blue', 'now')")
        .execute(&pool)
        .await
        .unwrap();
    assert!(!tag_suggest::current(&pool, "m1").await.unwrap().generated);
    sqlx::query("INSERT INTO tag_suggestions (meeting_id, names, generated_at) VALUES ('m1', '[\"acme\", \"Migration SQL\", \"Budget\"]', 'now')")
        .execute(&pool)
        .await
        .unwrap();
    let s = tag_suggest::current(&pool, "m1").await.unwrap();
    assert!(s.generated);
    assert_eq!(s.items.iter().map(|i| i.name.as_str()).collect::<Vec<_>>(), vec!["Acme", "Migration SQL", "Budget"]);

    // Accepting an existing tag reuses it; accepting a new one creates it with a colour
    let acme = tag_suggest::accept(&pool, "m1", "acme").await.unwrap();
    assert_eq!(acme.id, "a");
    let created = tag_suggest::accept(&pool, "m1", "Migration SQL").await.unwrap();
    assert!(created.id.starts_with("tag-"));
    assert_ne!(created.color, "default");
    tag_suggest::dismiss(&pool, "m1", "budget").await.unwrap();

    let names: Vec<String> = TagsRepository::for_meeting(&pool, "m1").await.unwrap().into_iter().map(|t| t.name).collect();
    assert_eq!(names, vec!["Acme", "Migration SQL"]);
    let s = tag_suggest::current(&pool, "m1").await.unwrap();
    assert!(s.generated && s.items.is_empty());
}
