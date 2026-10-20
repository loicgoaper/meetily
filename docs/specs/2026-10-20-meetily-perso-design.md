# Meetily perso : transcription cloud OpenRouter, diarisation, contexte, templates FR, vision écran, interface FR/EN

## Contexte

Loïc utilise Meetily 0.4.1 (Community, open source sous licence MIT) pour enregistrer ses réunions clients, en présentiel et en visio. Ce qu'il manque aujourd'hui :
- **Transcription** : elle est uniquement locale (Parakeet int8) et ne comprend pas assez bien le français.
- **Diarisation** : absente. Le micro et le son système sont mélangés avant la transcription, et la colonne `transcripts.speaker` n'est jamais remplie.
- **Contexte** : un simple champ texte, jamais sauvegardé, et injecté seulement dans le dernier appel au modèle.
- **Templates** : aucun écran pour les gérer (il faut déposer un fichier JSON dans `%APPDATA%\Meetily\templates`). Ils sont en anglais, et le compte rendu est d'abord écrit en anglais puis traduit.
- **Écran** : aucune capture.
- **Interface** : tous les textes sont écrits en dur en anglais, sans système de traduction.

Meetily Pro (120 $/an) n'apporterait que la diarisation, les templates personnalisés et le cloud, et son code est fermé. On construit donc **sa propre version** à partir du tag `v0.4.1`.

**Choix validés par Loïc** :
- Sa propre version.
- Transcription **tout en cloud, en direct**.
- Passe d'**identification des intervenants automatique à l'arrêt**, qui remplace le texte du direct.
- **Captures d'écran intelligentes** et vision.
- **Interface FR/EN** et contenus en français.
- Données actuelles **reprises, avec sauvegarde**.
- Livraison **en un seul bloc**.
- Templates FR : traduction des 6 existants, plus **Suivi de projet client** et **Réunion interne / équipe**.

Coût estimé par heure de réunion : environ 0,25 $ de transcription en direct, plus environ 0,25 $ de diarisation, plus 0,02 à 0,05 $ de vision, plus le modèle de résumé déjà utilisé.

## Ce qu'il faut savoir sur le code (vérifié)

- OpenRouter offre `POST /api/v1/audio/transcriptions` (JSON `{model, input_audio:{data,format}, language, response_format, timestamp_granularities, keyterms, diarize, provider}`).
  - 27 modèles disponibles : deepgram/nova-3, elevenlabs/scribe-v2, assemblyai/universal-3-6-pro, mistralai/voxtral-mini-transcribe-3-0, openai/gpt-4o-transcribe, whisper-large-v3-turbo, entre autres.
  - Limites : 25 Mo par envoi et 60 s de délai côté fournisseur. Pas de streaming : on envoie des morceaux d'audio.
- La clé OpenRouter est déjà dans `settings.openRouterApiKey`. On la réutilise pour la transcription et la vision.
- Mixage de l'audio : fenêtres de **600 ms** (`audio/pipeline.rs:41`, `AudioMixerRingBuffer`). La fenêtre i couvre exactement [0,6·i ; 0,6·(i+1)[ s sur l'axe de temps du VAD.
- `audio/transcription/provider.rs` définit déjà le trait `TranscriptionProvider`, et `engine.rs` déjà une variante `TranscriptionEngine::Provider`, mais elle n'est jamais construite.
- `worker.rs:74` : `NUM_WORKERS = 1`. Le `sequence_id` est attribué à la fin de la transcription (`worker.rs:193`), et non dans l'ordre de l'audio.
- `setting.rs:208` `get_transcript_api_key` renvoie une erreur pour un fournisseur inconnu, et Meetily retombe alors silencieusement sur Parakeet.
- Le résumé passe par `summary/processor.rs`, avec la consigne forcée « écris en anglais » puis un appel de traduction. `llm_client.rs` `ChatMessage{content: String}` n'accepte que du texte. OpenRouter n'est jamais découpé en morceaux (un seul appel final).
- Migrations : sqlx 0.8 avec `sqlx::migrate!` (`database/manager.rs:35`). Une migration absente ou modifiée fait planter l'app au démarrage (`.expect`).
- Le module de mise à jour automatique (updater) est actif, avec la clé et l'adresse officielles : il **écraserait notre version**.
- Machine : Rust 1.93, Node 24, pnpm 10, CMake et VS 2022 sont présents. **LLVM (libclang) manque.** Compte GitHub : `loicgoaper`.

## Étape 0 : version perso, compilation, sécurité des données

1. Créer le fork public `loicgoaper/meetily` sur GitHub (il contient le code seulement, aucune donnée). Le cloner dans `C:\Users\loicg\claude\meetily`, puis `git checkout -b fork/main v0.4.1`. Garder `upstream` pour les fusions futures. Recopier ce plan dans `docs/specs/2026-10-20-meetily-perso-design.md`.
2. Prérequis : installer LLVM (`winget install LLVM.LLVM`, puis `LIBCLANG_PATH`), vérifier le module « Desktop development with C++ » de VS, et activer `git config core.longpaths true`.
3. Compiler la v0.4.1 **sans modification**. Étapes : `cargo build --release -p llama-helper`, copier le résultat vers `frontend/src-tauri/binaries/llama-helper-x86_64-pc-windows-msvc.exe`, puis `pnpm install` et `pnpm tauri:build:cpu`. Le lancer sur une **copie** de la base pour valider la chaîne de compilation.
4. Dans `tauri.conf.json` : `createUpdaterArtifacts: false`, notre propre `pubkey` (`pnpm tauri signer generate`) et notre propre endpoint (releases du fork), `targets: ["nsis"]`, et une version `0.4.1-perso.1`. On **garde l'identifiant `com.meetily.ai`** pour reprendre les données.
5. Robustesse des migrations :
   - `m.set_ignore_missing(true)` dans `DatabaseManager::new`.
   - `cargo:rerun-if-changed=migrations` dans `build.rs`.
   - Migrations du fork nommées `2099…_fork_*.sql`, avec uniquement des `CREATE … IF NOT EXISTS`, sans jamais modifier un `.sql` officiel.
   - Retour arrière documenté : `DELETE FROM _sqlx_migrations WHERE version >= 20990000000000`.
6. Migration `20990101000001_fork_core.sql` : `app_preferences(key, value)` (tous les réglages du fork), `meeting_context`, `meeting_participants`, `contacts`, `meeting_speakers`, `meeting_usage`.
7. Mettre en place le système de traduction **dès maintenant** (voir l'étape 7), pour que tous les nouveaux écrans soient traduits d'office.

**Tests préalables (avant de coder les gros morceaux)** :
- **S1** : morceaux de 10, 20 et 30 s en français, envoyés à voxtral-mini-transcribe-3, gpt-4o-transcribe, nova-3 et whisper-large-v3-turbo. On mesure la latence (p50/p95), la qualité, l'acceptation de `keyterms` et la présence de `usage.cost`. Ce test détermine le **modèle par défaut pour le direct**.
- **S2** : audio d'une heure en opus 24 kb/s (environ 14,5 Mo en base64), envoyé à nova-3, scribe-v2 et assemblyai avec `diarize`. On vérifie les erreurs 413, le délai de 60 s et les champs `speaker`. Ce test détermine le **modèle de diarisation** et la taille des morceaux.
- **S3** : `xcap` sur les écrans de Loïc avec Teams ouvert, et `SetWindowDisplayAffinity` pour exclure la fenêtre Meetily des captures.

## Étape 1 : transcription OpenRouter en direct

- **À créer** :
  - `audio/transcription/openrouter_provider.rs` : `OpenRouterSttProvider` implémente `TranscriptionProvider`. Corps de requête JSON `{model, input_audio:{data(base64 wav), format:"wav"}, language:"fr", temperature:0, response_format:"json", keyterms, provider:{data_collection:"deny"}}`.
    - `keyterms` : noms des participants, sociétés et glossaire de la réunion (étape 2). En cas d'erreur 400, on relance une fois sans `keyterms` et on mémorise que ce modèle ne les accepte pas.
  - `audio/wav.rs` : encodage PCM16 mono (`base64` est déjà dans `Cargo.lock`).
  - `audio/transcription/cloud_chunker.rs` : regroupe les segments du VAD (cible 10 s, minimum 1,5 s, maximum 28 s). Le silence réel entre deux segments est comblé par des zéros pour garder les horodatages justes. Au-delà du maximum, découpe avec `audio/common.rs::split_segment_at_silence`.
- **À modifier** :
  - `provider.rs` : ajouter les méthodes par défaut `chunking_policy()` et `max_concurrency()`, pour que Whisper et Parakeet restent inchangés.
  - `worker.rs` : nombre de workers = `engine.max_concurrency()` (3 pour le cloud), et **`sequence_id` attribué à l'envoi** dans l'ordre de l'audio, ce qui corrige aussi l'ordre de récupération après plantage (IndexedDB). Filtre anti-hallucinations FR (« Sous-titres réalisés par… »).
  - `engine.rs` : branches `"openrouter"` dans `validate_transcription_model_ready` et `get_or_init_transcription_engine`, **avant** le cas par défaut `"localWhisper" | _`.
  - `setting.rs` : `"openrouter"` lit la clé de résumé (`get_api_key(pool, "openrouter")`), et la sauvegarde ne fait rien de plus.
  - `recording_commands.rs` (déchargement du modèle) et `recording_saver` (insertion triée par `audio_start_time`).
- **Résilience** :
  - 2 nouvelles tentatives (0,5 s puis 2 s).
  - Erreur 401/402/403 : alerte et bascule sur Parakeet local.
  - Disjoncteur : après 3 échecs de suite, 60 s en local, puis nouvel essai du cloud. Parakeet reste chargé en secours.
- **Coût** : lecture de `usage.cost`, événement `stt-usage`, enregistrement dans `meeting_usage`.
- **Re-transcription et import** (`retranscription.rs`, `import.rs`) : un enum `BatchEngine { Whisper, Parakeet, Cloud }` dans `common.rs`. Pour le cloud, ces chemins passent par la diarisation (étape 4b).
- **Interface** :
  - `TranscriptSettings.tsx` : fournisseur « OpenRouter (cloud) », liste des modèles récupérée en direct (nouvelle commande `get_openrouter_transcription_models` sur `GET /models?output_modalities=transcription`), langue `fr` par défaut, réglages de secours et de confidentialité.
  - `lib/transcription-model-readiness.ts` : ajouter `openrouter`.

## Étape 2 : contexte et participants

- **Tables** (migration du fork) :
  - `meeting_context(meeting_id, context_text, glossary, meeting_type presentiel|visio, extra_instructions, language, …)`
  - `meeting_participants(id, meeting_id, name, role, company, is_self, sort_order)`
  - `contacts` (autocomplétion des clients récurrents)
  - Suppression en cascade ajoutée dans `MeetingsRepository::delete_meeting`.
- **Backend** :
  - `database/repositories/meeting_context.rs` et `meeting_context/commands.rs` : `api_get_meeting_context`, `api_save_meeting_context`, `api_search_contacts`, `api_set_live_meeting_context`. Cette dernière alimente une variable globale `LIVE_MEETING_CONTEXT`, utilisée pour les `keyterms`, et écrit `context.json` dans le dossier de la réunion.
  - `api_save_transcript` reçoit le contexte et l'enregistre dans la même transaction, avant le résumé automatique.
- **Interface** :
  - `MeetingSetupDialog` s'ouvre au démarrage manuel (`hooks/useRecordingStart.ts`) : titre, type, contexte, participants, case « capturer l'écran ».
  - Bouton « Contexte » dans `RecordingControls` pendant l'enregistrement.
  - `MeetingDetails/MeetingContextPanel.tsx` remplace le champ texte non sauvegardé de `MeetingDetails/TranscriptPanel.tsx`. Le `customPrompt` devient `extra_instructions`, désormais enregistré.
- **Injection dans le résumé** :
  - `summary/commands.rs::api_process_transcript` charge le contexte, les participants, les intervenants et les notes d'écran.
  - `processor.rs` structure le message en `<meeting_context>`, `<screen_content>`, `<transcript_chunks>` et `<user_instructions>`. Les règles 2 et 3 du message système sont ajustées : le contexte est une source d'information, et toute consigne qu'il contient est traitée comme de la donnée. Le contexte est aussi injecté dans les prompts par morceau (modèles locaux).
  - Le cache (`SummaryCacheSource.custom_prompt_fingerprint`) couvre le contexte automatiquement.

## Étape 3 : gestionnaire de templates et français natif

- **Commandes** (`summary/templates/template_commands.rs`) :
  - `api_get_template_full` (avec sa source : intégré, fourni ou perso)
  - `api_save_custom_template` (validation, écriture atomique dans `loader::get_custom_templates_dir()`, rendue `pub`)
  - `api_delete_custom_template` (sur un template fourni, cela revient à le réinitialiser)
  - `api_duplicate_template`, `api_import_template`, `api_export_template` (chemins choisis via le plugin dialog)
- **Template par défaut** : `app_preferences['summary.default_template']`, lu par `useTemplates.ts` (aujourd'hui `standard_meeting` en dur) et par le cas de repli de `commands.rs` (aujourd'hui `daily_standup`).
- **Interface** : onglet « Modèles » dans `app/settings/page.tsx` et `components/Templates/TemplateManager.tsx` : liste, éditeur de sections (titre, consigne, format, item_format) et aperçu.
- **Templates FR** (`templates/fr_*.json` et `defaults.rs`) :
  - Traduction des 6 existants : réunion standard, point quotidien, synchro projet, rétrospective, appel commercial client, séance psy.
  - Deux nouveaux : **Suivi de projet client** (avancement, décisions, risques, actions | responsable | échéance | réf. [mm:ss]) et **Réunion interne / équipe**.
  - Section facultative « Éléments présentés à l'écran ».
- **Français natif** : dans `processor.rs`, la consigne « écris en anglais » devient `output_language_instruction(lang)`. Nouveau mode `FinalLanguageAction::Native` : un seul appel au modèle, sans traduction, et « Rien à signaler » pour une section vide. `SummaryCacheSource` gagne un champ `generation_mode`. La langue par défaut du résumé et de la transcription passe à `fr`.

## Étape 4a : « Moi » / « Interlocuteurs » en direct

- **À créer** : `audio/source_timeline.rs` enregistre l'énergie du micro et du son système pour chaque fenêtre de 600 ms. Ce relevé est fait dans `pipeline.rs`, juste avant `mix_window`.
  - `label(start, end)` renvoie `mic`, `system` ou `mixed` (seuil ±6 dB, en ignorant ce qui est sous −50 dBFS).
  - Fichier `source_energy.json` écrit à l'arrêt.
  - Le `cloud_chunker` coupe un morceau quand la source dominante change.
  - Étiquettes masquées en présentiel ou quand au moins 95 % de l'audio vient du micro.
- **Transmission du champ `speaker`** : `worker.rs` TranscriptUpdate → `recording_commands.rs` → `recording_saver` → `api::TranscriptSegment` → INSERT de `repositories/transcript.rs` → `models.rs` → `types/index.ts` / `TranscriptContext.tsx`.
- **Table `meeting_speakers(meeting_id, speaker_key, display_name, participant_id, source auto|manual|mic-anchor, confidence)`** : renommer un intervenant ne modifie qu'une ligne.
- **Interface** : pastille de nom d'intervenant et renommage dans `VirtualizedTranscriptView.tsx`. `useSummaryGeneration.ts` envoie au résumé des lignes `[mm:ss] Nom : texte`.

## Étape 4b : identification des intervenants à l'arrêt (automatique)

- **À créer** : `src/diarization/{mod,commands,align,naming}.rs` et la commande `api_diarize_meeting`. On réutilise le verrou de traitement en cours, l'annulation, les événements de progression et `find_audio_file` de `retranscription.rs`.
- **Déroulé** :
  1. ffmpeg (fourni avec l'app) convertit l'audio en mono 16 kHz opus 24 kb/s (mp3 32k en secours).
  2. Envoi à OpenRouter avec `verbose_json`, `diarize:true`, des timestamps au mot, `language:fr` et les `keyterms`. Morceaux de 5 à 10 min selon S2, qui se chevauchent de 20 s, 3 en parallèle. Les identifiants d'intervenants d'un morceau à l'autre sont réconciliés en comparant qui parle pendant le chevauchement.
  3. **Le texte du direct est remplacé** : reconstruction des segments à partir des mots, avec une coupure à chaque changement d'intervenant, chaque silence de plus de 1,2 s ou au-delà de 25 s. Le direct est sauvegardé dans `transcripts.live.json`, avec `api_revert_diarization` pour revenir en arrière. Écriture transactionnelle, comme `retranscription.rs`.
  4. « Moi » = l'intervenant qui parle à plus de 70 % dans le micro, d'après `source_energy.json`.
  5. Attribution des noms par le modèle de résumé (OpenRouter, réponse en JSON), à partir des participants, du contexte et d'extraits par intervenant. Le nom n'est appliqué que si la confiance est d'au moins 0,6, sinon « Intervenant n ». Un renommage manuel est toujours prioritaire.
- **Enchaînement** : arrêt → diarisation (réglage activé par défaut) → vision → résumé automatique. L'avancement s'affiche sur la page de la réunion.

## Étape 5 : captures d'écran intelligentes et vision

- **À créer** : les crates `xcap` et `image`, et `src/screen_capture/{mod,change_detect,commands}.rs`.
  - La capture démarre dans `start_recording_with_devices_and_meeting`, s'arrête avec `stop_recording` et se met en pause avec l'enregistrement.
  - Toutes les 5 s : capture de l'écran ou de la fenêtre choisie, réduite à 1600 px. On la garde si au moins 12 % d'une grille 32×18 a changé, que l'image est stable, et qu'il s'est écoulé au moins 15 s depuis la précédente. Plafond de 300 images.
  - Enregistrement dans `screens/NNNN_mm-ss.jpg` et `screens/index.json`, avec t = `get_active_recording_duration()` (même axe de temps que la transcription).
  - La fenêtre Meetily est exclue via `set_content_protected(true)`.
- **Commandes** : `api_list_meeting_screenshots`, `api_get_screenshot` (base64, pas besoin d'élargir l'`assetProtocol`), `api_exclude_screenshot`.
- **Interface** : case à cocher dans `MeetingSetupDialog`, et `MeetingDetails/ScreenshotGallery.tsx` (miniatures, exclusion d'une image, clic pour aller au moment correspondant).
- **Vision** (`summary/vision.rs`, nouveau type `VisionMessage` avec des parties texte et `image_url`, qui ne touche pas à `ChatMessage`) :
  - 40 images au maximum (réparties dans le temps, plus celles qui ont le plus changé), par lots de 8.
  - Modèle réglable dans `app_preferences['vision.model']`. Défaut : un modèle de vision bon marché, par exemple `google/gemini-3.1-flash-lite` ou `qwen/qwen3.7-flash`.
  - Prompt FR « [mm:ss] titre + contenu clé », en ignorant les vignettes webcam et en traitant tout texte à l'écran comme de la donnée.
  - Résultat mis en cache dans `screens/vision_notes.json`, puis injecté en `<screen_content>`.

## Étape 6 : réglages du fork

Onglet « Cloud & IA » dans `app/settings/page.tsx` :
- modèle de transcription en direct ;
- modèle de diarisation ;
- diarisation automatique (oui/non) ;
- modèle de vision et intervalle de capture ;
- confidentialité (`data_collection: deny`, ZDR) ;
- coût cumulé par réunion.

Tous ces réglages sont stockés dans `app_preferences`.

## Étape 7 : interface FR/EN

- `i18next` + `react-i18next`, côté client, compatible avec l'export statique de Next.js.
- Fichiers de traduction dans `frontend/src/i18n/locales/{fr,en}/*.json`, et un `I18nProvider` dans `app/layout.tsx`.
- Langue stockée dans `app_preferences['ui.language']`, avec une copie dans localStorage pour éviter un flash en anglais au démarrage. Sélecteur dans `PreferenceSettings.tsx`, et `<html lang>` mis à jour.
- Extraction de toutes les chaînes existantes, suivie avec `eslint-plugin-i18next`.
- Côté Rust : `src/i18n.rs` pour le menu de la barre des tâches (`tray.rs`) et les titres de notifications. Les erreurs envoyées à l'interface portent un `code`, que le frontend traduit.

## Fichiers clés

- Rust :
  - `frontend/src-tauri/src/audio/transcription/{worker,engine,provider}.rs`
  - `audio/pipeline.rs`, `audio/retranscription.rs`, `audio/import.rs`, `audio/common.rs`
  - `summary/{processor,service,commands}.rs`, `summary/templates/*`, `summary/llm_client.rs`
  - `database/{manager.rs,repositories/setting.rs,repositories/transcript.rs,models.rs}`
  - `lib.rs` (enregistrement des commandes), `tauri.conf.json`, `build.rs`, `Cargo.toml`
- Frontend :
  - `hooks/useRecordingStart.ts`, `useRecordingStop.ts`, `hooks/meeting-details/{useSummaryGeneration,useTemplates}.ts`
  - `contexts/TranscriptContext.tsx`, `ConfigContext.tsx`
  - `components/TranscriptSettings.tsx`, `components/MeetingDetails/*`, `app/settings/page.tsx`, `app/layout.tsx`

## Risques et parades

- **Fusions futures avec la version officielle** : le code nouveau va dans de nouveaux modules. Les fichiers officiels touchés restent peu nombreux, sauf pour la traduction de l'interface, qui touche presque tous les composants.
- **Latence en direct** : 3 workers en parallèle et un repli local. Les tests S1 fixent le modèle et la taille des morceaux.
- **Limites de 25 Mo / 60 s** : compression opus et découpage en morceaux qui se chevauchent (test S2).
- **Mise à jour automatique officielle** : neutralisée par notre propre clé `pubkey`.
- **Confidentialité** (données clients, RGPD / Loi 25) :
  - `data_collection: deny` par défaut, ZDR en option ;
  - capture d'écran activée réunion par réunion ;
  - exclusion d'images avant l'envoi à la vision ;
  - prévenir les participants de l'enregistrement.
- **Clés API en clair dans SQLite** : comportement existant, hors périmètre.

## Vérification

1. **Tests unitaires** (`cargo test`) :
   - en-tête WAV ;
   - `cloud_chunker` (regroupement, découpe, vidage sur inactivité, avec `tokio::time::pause`) ;
   - lecture des réponses OpenRouter (fichiers d'exemple json et verbose_json) ;
   - classement des erreurs pour les nouvelles tentatives ;
   - `source_timeline.label` ;
   - alignement et réconciliation des intervenants ;
   - détection de changement d'écran ;
   - validation, sauvegarde et suppression de templates ;
   - comparaison des prompts générés avec des versions de référence (contexte, langue FR native).
2. **Migrations** : lancer la version sur une **copie** de la vraie base (7 réunions). Vérifier que tout s'ouvre, que les anciens résumés s'affichent et que le retour arrière fonctionne.
3. **Scénarios dans l'app** (`pnpm tauri:dev:cpu`, puis build NSIS) :
   - Podcast FR de 20 min joué en son système : aucun morceau perdu, retard p95 sous 15 s, coût affiché.
   - Coupure réseau en pleine réunion : bascule sur Parakeet, puis retour au cloud.
   - Visio à 3 personnes (ou vidéo multi-voix) : « Moi » et « Interlocuteurs » en direct, puis à l'arrêt diarisation et noms attribués à partir des participants. Renommage manuel conservé.
   - Présentation PowerPoint partagée : captures uniquement aux changements de diapo, Meetily absent des captures, contenu des diapos repris dans le compte rendu avec les [mm:ss].
   - Templates : créer, dupliquer, modifier, importer, exporter, supprimer et définir par défaut. Compte rendu généré directement en français en un seul appel.
   - Basculer l'interface FR ↔ EN : aucun texte anglais restant en FR (`eslint-plugin-i18next`), menu de la barre des tâches compris.
4. **Installation finale** :
   - sauvegarde de `%APPDATA%\com.meetily.ai` et `Music\meetily-recordings` ;
   - désinstallation de la version officielle, installation de `0.4.1-perso.1` ;
   - les 7 réunions et la clé OpenRouter sont toujours là ;
   - aucune proposition de mise à jour officielle.
