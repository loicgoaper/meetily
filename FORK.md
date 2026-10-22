# Meetily perso

Version personnelle de [Meetily](https://github.com/Zackriya-Solutions/meetily) (licence MIT), construite à partir du tag `v0.4.1`. Branche de travail : `fork/main`. Le dépôt officiel reste configuré comme remote `upstream`.

## Ce qui change par rapport à la version officielle

| Fonction | Où | Détail |
|---|---|---|
| Transcription cloud en direct | Réglages → Transcription → OpenRouter (cloud) | Envoi de morceaux d'environ 10 s à `POST /api/v1/audio/transcriptions`, jusqu'à 3 en parallèle, repli sur Parakeet si le réseau tombe. Modèle par défaut : `elevenlabs/scribe-v2`. |
| « Moi » / « Interlocuteurs » en direct | Pendant l'enregistrement | Énergie du micro et du son système mesurée avant le mixage (fenêtres de 600 ms). |
| Identification des intervenants | Automatique à l'arrêt (désactivable) | L'audio complet est retranscrit avec diarisation (`elevenlabs/scribe-v2`, seul modèle qui l'accepte via OpenRouter), puis l'IA attribue les noms des participants. La transcription du direct est gardée dans `transcripts.live.json` et restaurable. |
| Contexte et participants | Fenêtre « Nouvelle réunion », bouton Contexte pendant l'enregistrement, panneau dans la réunion | Enregistrés en base. Injectés dans le compte rendu, l'attribution des noms et la transcription (vocabulaire pour les modèles `openai/*`). |
| Captures d'écran intelligentes | Case « Capturer l'écran » au démarrage | Une capture toutes les 5 s, gardée seulement si l'écran a changé et s'est stabilisé, en WebP (environ 60 Ko, un tiers du JPEG d'avant). Meetily est exclu des captures. Un modèle de vision lit **toutes** les captures, notes en français, avant le compte rendu. |
| Captures dans le compte rendu | Automatique après chaque compte rendu, bouton « Illustrer avec les captures » au-dessus du compte rendu | L'IA choisit jusqu'à 5 captures (réglable) qui appuient un point précis (diapositive, document, page produit, formulaire…) et Meetily les insère après ce point, avec une légende : `![légende](meetily-screen://FICHIER)`. Affichées dans l'éditeur, hébergées par Notion (API File Upload, chaque image envoyée une seule fois), jointes à l'export (dossier `captures/`), remplacées par leur légende dans la copie. |
| Conservation des captures | Réglages → Cloud & IA → Captures dans le compte rendu | Les captures absentes du compte rendu sont supprimées après 6 mois (3, 6, 12 mois ou jamais), seulement pour les réunions déjà illustrées. Les captures du compte rendu et les notes de vision sont gardées. Aperçu et nettoyage immédiat dans les réglages. |
| Modèles de compte rendu | Réglages → Modèles | Créer, modifier, dupliquer, importer, exporter, supprimer, choisir le modèle par défaut. 8 modèles en français, dont « Suivi de projet client » et « Réunion interne / équipe ». |
| Compte rendu en français natif | Automatique | Un seul appel au modèle, directement dans la langue demandée, au lieu d'un passage par l'anglais puis d'une traduction. |
| Interface FR/EN | Réglages → Général | Toute l'interface, le menu de la barre des tâches et les notifications. |
| Coût cloud | Barre au-dessus de la transcription d'une réunion | Somme des appels OpenRouter de la réunion (hors compte rendu). |
| Réécoute et correction des intervenants | Transcription d'une réunion : ▶ devant chaque ligne, clic sur le nom | Le bouton ▶ rejoue le passage (extrait mp3 produit par ffmpeg). Le menu du nom permet de renommer, ou d'indiquer que « Intervenant 4 » est en fait « Intervenant 1 », pour toutes ses lignes ou pour cette ligne seulement. |
| Export | Bouton « Exporter » d'une réunion | Compte rendu, transcription, contexte en Markdown, audio d'origine, captures. Un seul élément est enregistré tel quel, plusieurs sont réunis dans un zip. |
| Tags | Bouton « Ajouter un tag » d'une réunion, Réglages → Tags, ⚙ sous la recherche de la barre latérale | Tags personnalisés avec 10 couleurs (celles de Notion), créés à l'avance ou à la volée, renommés, recolorés, supprimés. Pastilles dans la barre latérale. Filtre sous la recherche : « Tout » par défaut, icône filtre pour chercher et cocher des tags dans une fenêtre, tags choisis sur deux lignes au plus (« +N » pour le reste), icône pour revenir à « Tout ». Une réunion doit porter tous les tags choisis. |
| Suggestions de tags | Barre de la réunion, à côté des tags | Après chaque compte rendu (ou à la première ouverture d'une réunion), l'IA propose jusqu'à 5 tags (client, projet, type de réunion, sujet) en réutilisant les tags existants. Un clic les ajoute, rien n'est appliqué sans accord. Désactivable dans Réglages → Tags. |
| Sauvegarde Notion | Réglages → Notion | Une base « Réunions Meetily » créée sous une page partagée avec l'intégration. Une page par réunion : propriétés (date, tags, participants, durée, coût), compte rendu, contexte, transcription dans un bloc dépliable. Envoi 15 s après chaque modification, et bouton « Tout synchroniser ». |
| Recherche IA | Bouton « Recherche IA » sous la recherche de la barre latérale | Question en langage naturel (« la réunion où l'on a parlé de SIRIS avec Olivier et Maxime »). L'IA extrait personnes, sujets et période, Meetily cherche dans toutes les transcriptions, puis l'IA classe les réunions trouvées et explique pourquoi. |
| Choix de l'écran capturé | Réglages → Cloud & IA → Écran capturé | Écran principal, un écran précis, ou tous les écrans (une capture par écran, numérotée). |

Le code ajouté vit dans `frontend/src-tauri/src/fork/` (Rust) et dans `frontend/src/components/fork`, `frontend/src/hooks/fork`, `frontend/src/services/forkService.ts` et `frontend/src/i18n` (interface). Les fichiers officiels ne reçoivent que de petits points d'accroche, ce qui facilite les fusions avec les versions futures.

## Clé Notion

Réglages → Notion. Meetily appelle lui-même l'API Notion, sans service intermédiaire. La clé est celle d'une intégration interne dédiée, nommée « Meetily » (notion.so/profile/integrations). Notion interdit à une clé d'intégration interne de créer une page à la racine de l'espace (seules les intégrations publiques OAuth le peuvent) : il faut donc connecter une page à l'intégration, menu « … » de la page → Plus → Connexions → Meetily. De retour dans Meetily, la liste se rafraîchit seule et présélectionne la page « Meetily » (ou la seule page connectée) : « Créer la base » suffit. En place depuis le 2026-10-21 : page privée « Meetily » contenant la base « Réunions Meetily ». La clé et les identifiants de la base sont dans `app_preferences` (`notion.token`, `notion.parent_page_id`, `notion.database_id`, `notion.auto_sync`), l'état de chaque réunion dans `notion_pages`.

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

Pour un installateur à copier sur un autre poste : `frontend\scripts\fork-build-installer.cmd`. Il compile, puis place `Meetily-perso-<version>-setup.exe` dans le dossier `installateur\` à la racine du dépôt (ignoré par git à cause de sa taille). `installateur\LISEZMOI.md` décrit l'installation et les réglages à refaire sur le nouveau poste.

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
   Les tables du fork (`app_preferences`, `meeting_context`, `meeting_participants`, `contacts`, `meeting_speakers`, `meeting_usage`, `tags`, `meeting_tags`, `notion_pages`, `tag_suggestions`, `notion_uploads`) peuvent rester : la version officielle les ignore.
3. Installer la version officielle.

## Fichiers ajoutés dans le dossier d'une réunion

| Fichier | Contenu |
|---|---|
| `source_energy.json` | Énergie micro / son système par fenêtre de 600 ms (identifie « Moi » après la diarisation) |
| `transcripts.live.json` | Transcription du direct, avant le remplacement par la diarisation |
| `context.json` | Contexte saisi pendant l'enregistrement (secours en cas de plantage) |
| `screens/NNNN_mm-ss.jpg`, `screens/index.json` | Captures d'écran gardées et leur horodatage (temps d'enregistrement actif) |
| `screens/vision_notes.json` | Ce que le modèle de vision a lu (langue de la réunion), réutilisé par capture tant que le modèle et la langue ne changent pas |
| `screens/illustration.json` | Date et nombre de captures de la dernière illustration du compte rendu (condition du nettoyage à 6 mois) |
| `screens/retention.json` | Dernier nettoyage : captures supprimées et place libérée |

## Confidentialité

Toutes les requêtes OpenRouter excluent les fournisseurs qui conservent ou réutilisent les données (`data_collection: deny`). L'option ZDR, dans Réglages → Cloud & IA, restreint encore aux fournisseurs qui ne conservent rien. Les captures d'écran ne sont prises que pour les réunions où la case est cochée, et chaque capture peut être exclue de l'analyse.

## Sécurité du contenu (CSP)

`tauri.conf.json` autorise `media-src 'self' data: blob:` (réécoute des lignes de transcription) et `ipc: http://ipc.localhost` dans `connect-src`. Sans cette dernière entrée, chaque appel Tauri était refusé puis rejoué par le canal de secours `postMessage`, plus lent.
