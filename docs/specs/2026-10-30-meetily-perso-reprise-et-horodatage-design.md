# Reprendre une réunion arrêtée, date et heure dans les comptes rendus

Version cible : **0.4.1-perso.8**. Conception validée par Loïc le 2026-10-30.

## Problèmes

1. **Une réunion arrêtée ne peut pas reprendre.** Si on arrête trop tôt, ou si la réunion repart après une pause, il faut créer une nouvelle réunion : deux transcriptions, deux comptes rendus, deux pages Notion.
2. **Le compte rendu ignore la date et l'heure.** Le modèle ne reçoit que des temps relatifs `[mm:ss]`. Il ne peut pas remplir les cases « Date, heure de début et de fin » des modèles (« Synchro projet », « Réunion client »…) ni écrire « vendredi 7 novembre » quand quelqu'un dit « vendredi prochain ».
3. **La date d'une réunion est son heure de fin.** `meetings.created_at` est l'heure de la sauvegarde, à l'arrêt. La liste, Notion et les exports affichent donc la fin, pas le début. Le vrai début est dans `metadata.json` (`created_at`, heure de lancement de l'enregistrement).

## Choix de Loïc

| Question | Choix |
|---|---|
| Que faire quand la partie reprise s'arrête ? | **Tout refaire** sur toute la réunion : intervenants, compte rendu, captures, tags, Notion. Les renommages manuels d'intervenants et les retouches du compte rendu sont remplacés. |
| Comment montrer la date et l'heure ? | **En-tête fixe** écrit par Meetily, **et** l'IA informée de la date. |
| Repères dans le compte rendu | **Temps écoulé `[23:40]`**, comme aujourd'hui. |

## 1. Reprendre une réunion

### Parcours

1. Page d'une réunion, barre du haut (`MeetingCloudBar`) : bouton **« Reprendre la réunion »**.
   - Grisé, avec une infobulle qui dit pourquoi, quand : un enregistrement est en cours, l'identification des intervenants ou la lecture des captures tourne, un compte rendu est en cours de génération, ou le dossier de la réunion est introuvable.
2. Le clic mène à l'écran d'enregistrement et ouvre la fenêtre « Nouvelle réunion » en **mode reprise** :
   - titre de la fenêtre « Reprendre « titre » », champ titre masqué ;
   - contexte (participants, contexte, glossaire, présentiel/visio, capture d'écran) prérempli avec celui de la réunion, modifiable ;
   - sources (micro, son de la réunion, écran) comme à un démarrage normal (perso.7) ;
   - phrase d'avertissement : « À l'arrêt, les intervenants et le compte rendu seront refaits sur toute la réunion. Les corrections manuelles seront remplacées. »
3. Pendant l'enregistrement, un bandeau indique « Reprise de « titre » ». La transcription en direct affiche des temps qui continuent ceux de la réunion.
4. À l'arrêt : sauvegarde dans **la même réunion**, ouverture de sa page, puis la chaîne de fin de réunion habituelle, forcée même si un compte rendu existe déjà.
5. On peut reprendre plusieurs fois, y compris un autre jour.

### Chronologie unique

La chronologie d'une réunion reste le **temps d'enregistrement actif** : pauses et interruptions exclues, comme aujourd'hui. La partie reprise commence au **décalage** `D` = durée de `audio.mp4` mesurée par ffmpeg au moment de la reprise. Si la réunion n'a pas d'audio, `D` = fin du dernier segment de transcription.

| Élément | Règle |
|---|---|
| Segments de transcription en direct | `audio_start_time` et `audio_end_time` + `D`. L'étiquette micro/système est calculée **avant** le décalage, sur la chronologie de la session. |
| Captures d'écran | `t` + `D`. Les numéros de fichier continuent (`0136_…`). La limite de 300 captures s'applique **par session**. |
| `source_energy.json` | fenêtres de la réunion complétées par des fenêtres muettes ou tronquées jusqu'à `D`, puis fenêtres de la session. |
| Audio | `audio.mp4` = concaténation (partie existante, puis reprise). |

### Fichiers : la partie existante n'est modifiée qu'après un recollage réussi

- La session reprise écrit **tout** dans `<dossier>/.reprise-N/`, `N` étant le prochain numéro libre : points de sauvegarde, `audio.mp4`, `transcripts.json`, `metadata.json`, `source_energy.json`. Rien n'est écrasé dans le dossier de la réunion.
- Les captures vont directement dans `<dossier>/screens/`, ajoutées à `index.json` comme aujourd'hui.
- À l'arrêt, après la sauvegarde normale de la session :
  1. **Audio** : concaténation ffmpeg (`-f concat -c copy`) vers `audio.reprise.tmp.mp4`. On vérifie que la durée obtenue vaut la somme des deux à 2 s près ; sinon on réessaie en réencodant (AAC). En cas de succès, le fichier remplace `audio.mp4` par renommage. En cas d'échec, `audio.mp4` reste intact, `.reprise-N/` est conservé et un événement `resume-merge-failed` affiche une erreur.
  2. **Repérage des sources** : fusion de `source_energy.json`.
  3. **`metadata.json`** de la réunion : ajout d'une liste `sessions` (`started_at`, `ended_at`, `offset_seconds`, `duration_seconds`). Si la liste est absente, la première session est reconstituée depuis `created_at`, `completed_at` et `duration_seconds`. `completed_at` et `duration_seconds` sont mis à jour. Tous les autres champs sont conservés, dont la langue du compte rendu.
  4. `.reprise-N/` est supprimé si tout a réussi.
- La réponse `recording-stopped` porte le dossier **de la réunion**.

### Sauvegarde dans la même réunion

- `api_save_transcript` reconnaît une reprise terminée dont le dossier correspond. Il **ajoute** alors les segments à la réunion existante (INSERT seulement, dans une transaction), met à jour `updated_at` et renvoie `{ meeting_id, resumed: true }`.
- Le contexte saisi dans la fenêtre de reprise et le coût de la transcription en direct sont enregistrés sur la réunion, comme pour une nouvelle réunion.
- Si `transcripts.live.json` existe (une identification a déjà eu lieu), les segments en direct de la reprise y sont **ajoutés**. « Restaurer la transcription du direct » rend alors les deux parties.
- Côté interface, l'arrêt saute la détection de langue pour une reprise : la langue de la réunion est déjà connue. Il ouvre la page avec `source=resume`.

### Chaîne de fin de réunion après une reprise

| Étape | Comportement |
|---|---|
| Identification des intervenants | Relancée sur l'audio complet : remplace toute la transcription et les noms. |
| Lecture des captures | Incrémentale : seules les nouvelles captures sont envoyées. |
| Compte rendu | Régénéré même s'il en existe un, si la génération automatique est activée. |
| Captures dans le compte rendu, tags, Notion | Refaits par le compte rendu, comme aujourd'hui. Même page Notion. |

Le verrou en mémoire de la chaîne (`processedMeetings`) est levé pour la réunion reprise.

### Erreurs et cas limites

- Démarrage refusé ou fenêtre annulée : la reprise est abandonnée (`api_cancel_resume`), rien n'a été écrit.
- Plantage pendant la reprise : la réunion d'origine est intacte. La session reste dans `.reprise-N/` et le prochain numéro libre sera utilisé. Pas de récupération automatique dans cette version.
- Réunion sans audio (enregistrement audio désactivé) : pas de recollage, décalage pris sur la transcription.

## 2. Date et heure

### Sessions d'une réunion (nouveau module `fork/meeting_time.rs`)

- La source est `metadata.json` du dossier : la liste `sessions` si elle existe, sinon une session reconstituée depuis `created_at`, `completed_at` et `duration_seconds`.
- Repli : `meetings.created_at` comme début, fin inconnue.
- Affichage dans le fuseau local de Windows. Le décalage UTC est donné au modèle.

### En-tête fixe

- Une ligne en italique, placée **en tête du compte rendu** juste avant son stockage, après l'insertion des captures :
  - `*Jeudi 30 octobre 2026 · 14 h 05 – 15 h 12 · 1 h 07*`
  - avec une reprise : `*Jeudi 30 octobre 2026 · 14 h 05 – 14 h 40, reprise 16 h 02 – 16 h 30 · 1 h 03*`
  - une reprise un autre jour : `… reprise le vendredi 31 octobre 09 h 00 – 09 h 30 …`
- La durée est le temps d'enregistrement actif total.
- La ligne est en français si le compte rendu est en français, sinon en anglais, au format 24 h (`Thursday 30 October 2026 · 14:05 – 15:12 · 1 h 07`).
- Le cache anglais (`english_cache`) ne la contient pas. L'export Markdown n'ajoute pas sa propre ligne de date quand le compte rendu commence déjà par cet en-tête.

### Ce que reçoit le modèle

En tête du bloc `<meeting_context>`, qui est désormais toujours présent :

```
Meeting date: jeudi 30 octobre 2026 (2026-10-30), local time UTC+01:00
Session 1: 14:05–14:40, transcript [00:00]–[35:12]
Session 2 (resumed): 16:02–16:30, transcript [35:12]–[63:40]
Transcript times [mm:ss] are elapsed recording time, not clock time.
```

La règle 4 du prompt final ajoute :
- utiliser cette date et ces heures pour tout champ de date ou d'heure du modèle ;
- convertir les dates relatives (« demain », « vendredi prochain ») en dates absolues ;
- garder `[mm:ss]` pour les repères.

Cela vaut aussi pour la régénération d'un ancien compte rendu.

### Date d'une réunion = heure de début

- **Nouvelles réunions** : à la sauvegarde, `meetings.created_at` prend le `created_at` de `metadata.json`.
- **Anciennes réunions** : correction unique au démarrage, notée par la préférence `fork.created_at_from_metadata = done`. Garde-fous :
  - seulement si le début lu dans `metadata.json` est antérieur à la valeur en base de moins de 12 h ;
  - sauvegarde de la base au préalable (`~/.meetily-perso/backups`).
- La liste, Notion (propriété Date) et les exports suivent sans autre changement.

## Tests

**Rust**
- `meeting_time` : sessions lues ou reconstituées, en-tête FR/EN (même jour, autre jour, sans fin), bloc du prompt, durée.
- `resume` : décalage, prochain `N` libre, fusion de `source_energy`, mise à jour de `metadata.json` avec les champs inconnus conservés, plan de concaténation et vérification de durée (ffmpeg réel sur deux fichiers générés), ajout à `transcripts.live.json`.
- Base de données : ajout de segments à une réunion existante, correction de `created_at` avec ses garde-fous.
- Prompt : règle de date présente, `<meeting_context>` toujours présent.

**Interface**
- bun : clés de traduction FR/EN, type `resumed` de la réponse, choix de `source=resume`.

**Réel, sur le PC de Loïc, sans vraie réunion**
- D'abord : vérifier qu'aucun enregistrement n'est en cours.
- Réunion de test de 1 min, avec un dialogue synthétique joué par les haut-parleurs et la capture d'écran, puis arrêt, chaîne complète, reprise de 1 min, arrêt.
- Points contrôlés :
  - `audio.mp4` d'environ 2 min ;
  - temps continus dans la transcription ;
  - captures numérotées à la suite avec `t` > `D` ;
  - en-tête avec les deux sessions ;
  - compte rendu régénéré ;
  - page Notion mise à jour, puis réunion de test supprimée.

## Hors périmètre

- Reprendre automatiquement une session interrompue par un plantage.
- Afficher la transcription précédente pendant la reprise.
- Conserver les noms d'intervenants corrigés à la main d'une identification à l'autre.
