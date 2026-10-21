# Meetily perso

Version personnelle de [Meetily](https://github.com/Zackriya-Solutions/meetily) (licence MIT), construite à partir du tag `v0.4.1`. Branche de travail : `fork/main`. Le dépôt officiel reste configuré comme remote `upstream`.

## Ce qui change par rapport à la version officielle

| Fonction | Où | Détail |
|---|---|---|
| Transcription cloud en direct | Réglages → Transcription → OpenRouter (cloud) | Envoi de morceaux d'environ 10 s à `POST /api/v1/audio/transcriptions`, jusqu'à 3 en parallèle, repli sur Parakeet si le réseau tombe. Modèle par défaut : `elevenlabs/scribe-v2`. |
| « Moi » / « Interlocuteurs » en direct | Pendant l'enregistrement | Énergie du micro et du son système mesurée avant le mixage (fenêtres de 600 ms). |
| Identification des intervenants | Automatique à l'arrêt (désactivable) | L'audio complet est retranscrit avec diarisation (`elevenlabs/scribe-v2`, seul modèle qui l'accepte via OpenRouter), puis l'IA attribue les noms des participants. La transcription du direct est gardée dans `transcripts.live.json` et restaurable. |
| Contexte et participants | Fenêtre « Nouvelle réunion », bouton Contexte pendant l'enregistrement, panneau dans la réunion | Enregistrés en base. Injectés dans le compte rendu, l'attribution des noms et la transcription (vocabulaire pour les modèles `openai/*`). |
| Captures d'écran intelligentes | Case « Capturer l'écran » au démarrage | Une capture toutes les 5 s, gardée seulement si l'écran a changé et s'est stabilisé. Meetily est exclu des captures. Un modèle de vision les lit avant le compte rendu. |
| Modèles de compte rendu | Réglages → Modèles | Créer, modifier, dupliquer, importer, exporter, supprimer, choisir le modèle par défaut. 8 modèles en français, dont « Suivi de projet client » et « Réunion interne / équipe ». |
| Compte rendu en français natif | Automatique | Un seul appel au modèle, directement dans la langue demandée, au lieu d'un passage par l'anglais puis d'une traduction. |
| Interface FR/EN | Réglages → Général | Toute l'interface, le menu de la barre des tâches et les notifications. |
| Coût cloud | Barre au-dessus de la transcription d'une réunion | Somme des appels OpenRouter de la réunion (hors compte rendu). |
| Réécoute et correction des intervenants | Transcription d'une réunion : ▶ devant chaque ligne, clic sur le nom | Le bouton ▶ rejoue le passage (extrait mp3 produit par ffmpeg). Le menu du nom permet de renommer, ou d'indiquer que « Intervenant 4 » est en fait « Intervenant 1 », pour toutes ses lignes ou pour cette ligne seulement. |
| Export | Bouton « Exporter » d'une réunion | Compte rendu, transcription, contexte en Markdown, audio d'origine, captures. Un seul élément est enregistré tel quel, plusieurs sont réunis dans un zip. |
| Tags | Bouton « Ajouter un tag » d'une réunion, Réglages → Général → Tags | Tags personnalisés avec 10 couleurs (celles de Notion). Pastilles dans la barre latérale, filtre par tag sous la recherche (une réunion doit porter tous les tags choisis). |
| Sauvegarde Notion | Réglages → Notion | Une base « Réunions Meetily » créée sous une page partagée avec l'intégration. Une page par réunion : propriétés (date, tags, participants, durée, coût), compte rendu, contexte, transcription dans un bloc dépliable. Envoi 15 s après chaque modification, et bouton « Tout synchroniser ». |
| Recherche IA | Bouton « Recherche IA » sous la recherche de la barre latérale | Question en langage naturel (« la réunion où l'on a parlé de SIRIS avec Olivier et Maxime »). L'IA extrait personnes, sujets et période, Meetily cherche dans toutes les transcriptions, puis l'IA classe les réunions trouvées et explique pourquoi. |
| Choix de l'écran capturé | Réglages → Cloud & IA → Écran capturé | Écran principal, un écran précis, ou tous les écrans (une capture par écran, numérotée). |

Le code ajouté vit dans `frontend/src-tauri/src/fork/` (Rust) et dans `frontend/src/components/fork`, `frontend/src/hooks/fork`, `frontend/src/services/forkService.ts` et `frontend/src/i18n` (interface). Les fichiers officiels ne reçoivent que de petits points d'accroche, ce qui facilite les fusions avec les versions futures.

## Clé Notion

Réglages → Notion. La clé est celle d'une intégration interne (notion.so/my-integrations). Pour que Meetily puisse créer sa base, il faut partager une page avec l'intégration dans Notion : menu « … » de la page → Connexions → ajouter l'intégration. La page apparaît ensuite dans la liste des pages parentes. La clé et les identifiants de la base sont dans `app_preferences` (`notion.token`, `notion.parent_page_id`, `notion.database_id`, `notion.auto_sync`), l'état de chaque réunion dans `notion_pages`.

## Clé OpenRouter

Une seule clé sert à tout : compte rendu, transcription, intervenants et vision. On la change dans Réglages → Cloud & IA, ou dans Réglages → Transcription → OpenRouter. Elle est stockée dans la table `settings` (colonne `openRouterApiKey`), comme dans la version officielle.

## Compiler sous Windows

Prérequis, à installer une seule fois :

- Visual Studio Build Tools 2022, charge de travail « Développement Desktop en C++ »
- CMake
- **LLVM 18.x** (`winget install LLVM.LLVM --version 18.1.8`). La version 22 génère une structure `whisper_full_params` opaque avec le bindgen 0.69 de `whisper-rs-sys`, et la compilation échoue.
- Rust (MSVC), Node 20+ et pnpm

Toutes les commandes cargo et tauri passent par `frontend/scripts/fork-dev-env.cmd`. Ce script charge l'environnement MSVC (`vcvars64`) et règle `LIBCLANG_PATH` et `BINDGEN_EXTRA_CLANG_ARGS` :

```powershell
cd frontend
# llama-helper (assistant local de compte rendu), une fois
cmd /c "scripts\fork-dev-env.cmd cargo build --release -p llama-helper --manifest-path ..\Cargo.toml"
copy ..\target\release\llama-helper.exe src-tauri\binaries\llama-helper-x86_64-pc-windows-msvc.exe

pnpm install
cmd /c "scripts\fork-dev-env.cmd pnpm tauri:dev:cpu"     # développement
cmd /c "scripts\fork-dev-env.cmd pnpm tauri:build:cpu"   # installateur NSIS
```

Tests : `cmd /c "scripts\fork-dev-env.cmd cargo test --lib"` (dans `src-tauri`), `node scripts/check-i18n-keys.mjs` et `npx tsc --noEmit -p tsconfig.json` (dans `frontend`).

## Données et mises à jour

- L'identifiant d'application reste `com.meetily.ai` : la version perso reprend la base `%APPDATA%\com.meetily.ai\meeting_minutes.sqlite`, les modèles et les réunions existantes.
- La mise à jour automatique officielle est neutralisée par une clé de signature propre au fork, qui se trouve dans `C:\Users\loicg\.meetily-perso\updater.key`. L'adresse de mise à jour pointe vers les releases de `loicgoaper/meetily`.
- Les migrations du fork s'appellent `2099…_fork_*.sql` et ne font que des `CREATE … IF NOT EXISTS`. L'app tolère aussi les migrations qu'elle ne connaît pas (`set_ignore_missing`).

### Revenir à la version officielle

1. Fermer Meetily et sauvegarder `%APPDATA%\com.meetily.ai\meeting_minutes.sqlite`.
2. Retirer les migrations du fork de l'historique, pour que la version officielle accepte d'ouvrir la base :
   ```sql
   DELETE FROM _sqlx_migrations WHERE version >= 20990000000000;
   ```
   Les tables du fork (`app_preferences`, `meeting_context`, `meeting_participants`, `contacts`, `meeting_speakers`, `meeting_usage`, `tags`, `meeting_tags`, `notion_pages`) peuvent rester : la version officielle les ignore.
3. Installer la version officielle.

## Fichiers ajoutés dans le dossier d'une réunion

| Fichier | Contenu |
|---|---|
| `source_energy.json` | Énergie micro / son système par fenêtre de 600 ms (identifie « Moi » après la diarisation) |
| `transcripts.live.json` | Transcription du direct, avant le remplacement par la diarisation |
| `context.json` | Contexte saisi pendant l'enregistrement (secours en cas de plantage) |
| `screens/NNNN_mm-ss.jpg`, `screens/index.json` | Captures d'écran gardées et leur horodatage (temps d'enregistrement actif) |
| `screens/vision_notes.json` | Ce que le modèle de vision a lu, mis en cache par modèle et par jeu de captures |

## Confidentialité

Toutes les requêtes OpenRouter excluent les fournisseurs qui conservent ou réutilisent les données (`data_collection: deny`). L'option ZDR, dans Réglages → Cloud & IA, restreint encore aux fournisseurs qui ne conservent rien. Les captures d'écran ne sont prises que pour les réunions où la case est cochée, et chaque capture peut être exclue de l'analyse.
