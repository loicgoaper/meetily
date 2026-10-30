# PRD brownfield — Epic 7 : sources de l'enregistrement

| Version | Date | Auteur | Changement |
|---|---|---|---|
| 1.0 | 2026-10-28 | PM (Claude) | Création après la séance du Conseil du 28/10 (`docs/conseil/`) |

## 1. Contexte et problème

Meetily perso enregistre des réunions sous Windows : micro, son de la réunion (sortie audio en boucle) et, sur demande, captures d'écran.

**Aujourd'hui, au démarrage d'une réunion, rien ne se choisit :**
- le micro et la sortie audio viennent des Réglages, sinon des périphériques Windows par défaut ;
- l'écran capturé vient d'un réglage global.

**Incident du 23/10 :** deux réunions Teams ont été enregistrées sans la voix des interlocuteurs. Meetily écoutait les haut-parleurs du portable alors que Teams jouait dans le casque Jabra. La perso.6 a corrigé le choix automatique de la sortie et ajouté une alerte après 30 s de silence. Mais l'utilisateur ne peut toujours ni choisir ni vérifier ses sources au moment de démarrer.

## 2. Objectifs

- **O1 :** aucune réunion en visio enregistrée sans la voix des interlocuteurs. Mesure : fenêtres « système » actives dans `source_energy.json` pour 100 % des réunions visio.
- **O2 :** l'utilisateur choisit, à chaque démarrage, le micro, le son de la réunion et l'écran (principal, tous, ou l'écran n), en moins de 10 secondes.
- **O3 :** avant de cliquer sur « Démarrer », l'utilisateur voit l'appareil qui sera réellement enregistré et son niveau en direct.

**Non-objectifs (carnet de commandes) :**
- changer de source pendant l'enregistrement ;
- préréglages par type de réunion ;
- capture par application (story 7.5, essai à part).

## 3. Utilisateur et usages

Un consultant seul, sous Windows 11, avec :
- un casque Jabra, des haut-parleurs Realtek, 2 écrans V277U et un micro intégré ;
- des visios Teams, et des réunions en présentiel ;
- un installateur qui équipe aussi d'autres postes.

## 4. Exigences fonctionnelles

- **FR1 :** la fenêtre « Nouvelle réunion » contient une section « Sources de l'enregistrement » : micro, son de la réunion, et écran capturé (affiché seulement si « Capturer l'écran » est coché).
- **FR2 (règle du Conseil) :** le son de la réunion est proposé en « Automatique » à **chaque** démarrage. Un choix manuel ne vaut que pour la réunion en cours et n'est **jamais mémorisé**.
- **FR3 :** le micro et l'écran choisis sont mémorisés et reproposés à la réunion suivante.
- **FR4 :** un appareil mémorisé mais absent n'est jamais envoyé tel quel. La fenêtre affiche « X non branché : Y sera utilisé ». Côté moteur, une sortie demandée mais introuvable bascule sur la sortie de la réunion (règle automatique), pas sur la sortie multimédia.
- **FR5 :** « Automatique » affiche l'appareil qu'il retient (« Automatique · Casque Jabra »). Une jauge en direct accompagne le micro et le son de la réunion.
- **FR6 :** les écrans sont identifiables :
  - miniature de chaque écran ;
  - numéro attribué selon la position, de gauche à droite ;
  - « principal » signalé ;
  - choix mémorisé par position, pas par identifiant système.
- **FR7 :** les captures de « Tous les écrans » portent le même numéro d'écran que la liste.
- **FR8 :** en présentiel, la ligne « Son de la réunion » est repliée (Automatique) et reste modifiable.
- **FR9 :** l'alerte de silence dit quoi faire : arrêter, puis relancer en choisissant l'appareil proposé dans « Son de la réunion ».

## 5. Exigences non fonctionnelles

- **NFR1 :** aucune régression des démarrages existants (barre des tâches, raccourci, démarrage automatique) : sans choix, comportement de la perso.6.
- **NFR2 :** lire les niveaux coûte moins de 50 ms par lecture et 300 ms entre deux lectures. Aucun flux audio n'est ouvert pour cela (compteurs de crête Windows).
- **NFR3 :** les miniatures d'écran sont produites en moins d'une seconde et ne sont jamais enregistrées sur disque.
- **NFR4 :** interface en français et en anglais (contrôle des clés), et code testé (tests Rust et bun).

## 6. Hypothèses techniques

- **Niveaux :** compteurs de crête Core Audio (`IAudioMeterInformation`) sur chaque point de sortie et d'entrée actif, crate `windows` 0.61, déjà utilisé par `fork/audio_route.rs`. Pour un micro, le compteur ne bouge que si une application l'utilise déjà (Teams en appel), ce qui est acceptable.
- **Écrans :** xcap 0.9.8 (`x()`, `y()`, `width()`, `height()`, `is_primary()`, `capture_image()`). Son `id()` est un handle Windows : il ne sert pas de clé durable.
- **Choix d'écran propre à la réunion :** champ `screen_monitor` du contexte de réunion en cours, sans persistance, lu par `fork/screen_capture.rs`.

## 7. Epic 7 — stories

| Story | Titre | Dépend de |
|---|---|---|
| 7.1 | Micro et son de la réunion au démarrage, règles de mémorisation et appareils absents | — |
| 7.2 | Appareil retenu et niveaux en direct | 7.1 |
| 7.3 | Écrans identifiables (miniatures, position, clé stable) | 7.1 |
| 7.4 | Présentiel et alerte qui dit quoi faire | 7.1 |
| 7.5 | Essai : capture par application (process loopback), plafonné à une demi-journée | après le MVP |

Les stories détaillées sont dans `docs/bmad/stories/`.

## 8. Recette du MVP

Trois essais réels de 2 minutes :
1. Teams dans le Jabra, son en Automatique.
2. Micro Intel forcé.
3. Écran 2 seul.

On vérifie le fichier audio (deux sources actives), les captures et les jauges. Puis : installateur perso.7, installation hors réunion (vérifier `.checkpoints` et les comptes rendus en attente).
