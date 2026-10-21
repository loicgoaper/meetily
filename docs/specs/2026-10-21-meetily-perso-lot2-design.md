# Meetily perso – lot 2 (2026-10-21)

Validé par Loïc le 21/10/2026.

1. **Réécoute et correction des intervenants**
   - `api_get_audio_clip(meeting, start, end)` : ffmpeg extrait le passage (±0,5 s) en mp3 et le renvoie en data URL.
   - Bouton ▶ sur chaque ligne de transcription.
   - Menu de l'intervenant : écouter un extrait, renommer, rattacher à un autre intervenant, soit pour toutes ses phrases (`api_merge_speakers`), soit pour cette phrase (`api_set_segment_speaker`).
2. **Export**
   - `api_export_meeting(meeting, parts)` accepte : audio, transcript.md, compte-rendu.md, contexte.md, captures.
   - Plusieurs éléments donnent un zip, un seul élément donne directement le fichier. Le chemin est choisi dans la boîte de dialogue Enregistrer.
3. **Tags**
   - Tables `tags(id, name, color)` et `meeting_tags`. Les couleurs reprennent la palette Notion (default, gray, brown, orange, yellow, green, blue, purple, pink, red).
   - Sélecteur dans la barre de la réunion, pastilles et filtre multi-tags dans la barre latérale, gestion dans les Réglages.
4. **Notion** (synchronisation à sens unique, de l'app vers Notion)
   - Jeton dans `app_preferences['notion.token']`.
   - Base « Réunions Meetily » créée sous une page partagée avec l'intégration. Colonnes : Nom, Date, Tags, Participants, Durée (min), Coût cloud ($), Meetily ID.
   - Une page par réunion : infos, compte rendu en blocs, transcription dans un titre dépliable.
   - Mise à jour en place : propriétés, puis remplacement des blocs. Rien n'est renvoyé si le contenu n'a pas changé (empreinte).
   - Une file « à synchroniser » est alimentée par le compte rendu, la diarisation, les intervenants, les tags et le contexte. Un worker la traite 15 s après la dernière modification.
   - Un bouton « Tout synchroniser ».
   - Table `notion_pages(meeting_id, page_id, content_hash, synced_at, last_error)`.
5. **Recherche IA** (`api_ai_search(query)`)
   1. Le LLM extrait du texte de la recherche les personnes, les sujets et les dates.
   2. Ces termes sont cherchés dans les transcriptions, sans tenir compte des accents ni des majuscules, ainsi que dans les participants, les intervenants, les tags et les titres.
   3. Le LLM classe des fiches résumées de toutes les réunions et renvoie une justification avec les [mm:ss].
6. **Captures d'écran**
   - La préférence `screen.monitor` accepte `primary`, `monitor:<id>` ou `all`.
   - Avec `all`, chaque écran a son propre détecteur, ses fichiers `…_e<n>.jpg` et un champ `monitor` dans l'index.
