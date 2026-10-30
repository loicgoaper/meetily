# Choix des sources d'enregistrement au démarrage d'une réunion

- **Date** : 2026-10-28
- **Statut** : En vigueur
- **Projet** : C:\Users\loicg\claude\meetily

## Sujet soumis

Décision produit pour le fork Meetily perso : choix des sources d'enregistrement au démarrage d'une réunion (micro, son de la réunion, écran(s) capturé(s)).

## Dossier remis au Conseil

_L'Étranger a reçu ce dossier sans le paragraphe « Archives du Conseil »._

**Sujet** (mot pour mot) : « on as un soucis le choix du micro et du son interne n'est pas personnalisable au demarrage de la reunion ni le choix de l'ecran à enregistrer tous un 1 ou 2 ou 3 ou x suivant le nombre d'ecran que gére le pc » — puis : « utilise la methode bmad avec prd et story comme un vrai dev d'app profesionnel et utiliser la skill conceil pour faire les meilleur choix enfonction de l'objectif ».

**Contexte factuel** : Projet : version personnelle de Meetily (enregistreur de réunions, Tauri v2 + Next.js), Windows, dépôt C:\Users\loicg\claude\meetily, un seul utilisateur (consultant, visios Teams surtout, aussi présentiel), installateur pour d'autres postes. Au démarrage, la fenêtre « Nouvelle réunion » (frontend/src/contexts/MeetingSetupContext.tsx) demande titre, type présentiel/visio, contexte, participants, case « capturer l'écran ». Micro et sortie audio se règlent seulement dans Réglages → Enregistrements (frontend/src/components/DeviceSelection.tsx) ; sans choix : périphériques Windows par défaut. Depuis la v perso.6, sortie choisie automatiquement : celle où le son joue, sinon la sortie « communications » (frontend/src-tauri/src/fork/audio_route.rs), plus une alerte après 30 s de silence système. L'écran capturé se règle dans Réglages → Cloud & IA (principal / tous / un écran précis), lu au début de la capture (frontend/src-tauri/src/fork/screen_capture.rs). Incident du 23/10 : deux enregistrements sans le son des interlocuteurs (Meetily écoutait les haut-parleurs du portable, Teams jouait dans le casque Jabra). Poste : sorties Realtek, casque Jabra, 2 écrans V277U (audio HDMI), câble virtuel VB-Audio ; micro intégré Intel ; plusieurs écrans. Un brouillon non validé existe : section « Sources de l'enregistrement » dans la fenêtre de démarrage (3 listes : micro, son de la réunion « automatique » ou appareil précis, écran si capture cochée), choix appliqués à la réunion et mémorisés pour la suivante (frontend/src/components/fork/RecordingSourcesPicker.tsx, frontend/src/hooks/useRecordingStart.ts). Questions ouvertes : périmètre du MVP ; mémoriser ou non ; affichage selon présentiel/visio ; changement en cours d'enregistrement ; identification des écrans ; test de niveau avant de démarrer ; découpage en stories.

**Archives du Conseil** : C:\Users\loicg\claude\meetily\docs\conseil\ — aucune décision antérieure.

## Positions

| Membre | Position |
|---|---|
| Le Contre-Expert | Mémoriser une sortie « désactive la détection automatique de la perso.6 » ; le brouillon envoie même un appareil débranché, à l'insu de l'utilisateur. |
| L'Architecte | « On résout le mauvais problème » ; la capture par application de Windows (process loopback) prendrait Teams quelle que soit la sortie ; essai d'abord. |
| L'Optimiste | Le niveau des sorties est déjà lu dans `audio_route.rs` ; une jauge par source rend l'erreur du 23/10 visible avant de démarrer. |
| L'Étranger | « Choisir sans voir ni entendre, c'est deviner » ; l'essentiel est de savoir en quelques secondes que la voix des autres arrive. |
| L'Exécutant | Livrer le brouillon corrigé en 48 h : sortie jamais mémorisée, écrans identifiables, recette réelle ; pas de test de niveau ni de bascule en direct dans le MVP. |

## Arbitrages

**Vérifications du Président** : confirmés dans le code — `startWith` envoie l'appareil mémorisé brut même débranché ; `windows.rs:239-245` retombe alors sur la sortie multimédia ; `windows.rs:192` fait une correspondance par sous-chaîne ; xcap 0.9.8 `id()` = `h_monitor.0 as u32` (handle).

### Mémoriser ou non la sortie « son de la réunion »

- **Brouillon** : mémoriser la sortie choisie.
- **Le Contre-Expert, l'Exécutant, l'Étranger** : ne jamais la mémoriser.
- **Arbitrage du Président** : la sortie « son de la réunion » n'est jamais mémorisée ; « Automatique » est proposé à chaque démarrage ; un choix manuel vaut pour cette réunion seulement. Micro et écran peuvent être mémorisés. — Fait vérifié : une sortie nommée passe toujours avant l'automatique, et une sortie débranchée retombe en silence sur la multimédia.

### Test de niveau dans le MVP

- **L'Étranger, l'Optimiste** : test de niveau dans le MVP.
- **L'Exécutant** : test de niveau hors MVP.
- **L'Architecte** : avant l'appel, rien ne joue.
- **Arbitrage du Président** : dans le MVP, sous la forme d'une ligne « appareil réellement enregistré + niveau en direct », seule vérification avant de démarrer, et peu coûteuse. — On démarre souvent Meetily pendant l'appel, et la lecture du niveau existe déjà.

### Capture par application d'abord ou choix d'appareil d'abord

- **L'Architecte** : capture par application d'abord.
- **L'Exécutant** : choix d'appareil d'abord.
- **Arbitrage du Président** : story à part, plafonnée à une demi-journée, juste après le MVP ; si elle marche, elle devient le moteur de « Automatique ». Le choix du micro et de l'écran est utile quoi qu'il arrive, donc ne l'attend pas. — Piste structurelle plausible mais non prouvée sur ce poste (nouveau Teams en WebView2).

## Verdict

- **On garde** : la section « Sources de l'enregistrement » dans la fenêtre de démarrage ; le choix de l'écran pour la réunion lu côté Rust ; la mémorisation du micro et de l'écran ; l'alerte des 30 s.
- **On jette** : la mémorisation de la sortie « son de la réunion » ; la numérotation des écrans selon l'ordre de xcap ; l'envoi silencieux d'un appareil débranché.
- **On modifie** : afficher l'appareil réellement enregistré (« Automatique · Casque Jabra ») avec son niveau en direct ; signaler en clair un appareil absent ; écrans identifiables (miniature + position gauche → droite), mémorisés par position et non par handle ; en présentiel, replier la ligne du son de la réunion ; PRD BMAD court, stories avec critères d'acceptation.

Le brouillon va dans la bonne direction mais, tel quel, il peut recréer le 23/10 sans que personne le voie. Le MVP doit permettre de choisir, de voir ce qui sera enregistré, et ne jamais figer une sortie devenue fausse. Le changement de source en cours d'enregistrement va au carnet de commandes.

**Décisions archivées remplacées** : aucune (archives vides).

## Plan de bataille

1. PRD brownfield court et stories (Epic 7) avec critères d'acceptation, dans `docs/bmad/`.
2. Story 7.1 : micro et son de la réunion au démarrage ; sortie jamais mémorisée ; appareils périmés nettoyés ; avertissement côté Rust si un appareil demandé est absent.
3. Story 7.2 : appareil résolu et niveau en direct dans la fenêtre.
4. Story 7.3 : écrans identifiables (position, miniatures, clé stable).
5. Story 7.4 : présentiel, et alerte qui dit quoi faire.
6. QA : tests unitaires, recette réelle (Teams dans le Jabra en automatique, micro forcé, écran 2 seul), installateur perso.7, installation hors réunion.
7. Story 7.5 (essai, une demi-journée) : capture par application.

## Rapports des membres

### Le Contre-Expert

**Position** : le brouillon, tel qu'il est, recrée l'incident du 23/10 de façon silencieuse. Le danger n'est pas le périmètre du MVP, c'est la mémorisation d'une sortie audio précise. Archives vides, aucun risque passé à vérifier.

**Arguments**
1. **Mémoriser une sortie précise désactive la détection automatique de la perso.6** — critique, probabilité élevée. `recording_commands.rs:244-248` : un appareil nommé gagne toujours ; `meeting_output()` (`:259`) ne joue que sans choix. Un seul choix « Jabra » et la visio suivante où Teams joue ailleurs enregistre du silence. Signal : log « Using requested system audio » puis événement `system-audio-silent`.
2. **Bug vérifié : l'écran affiche « Automatique » mais le code envoie un appareil débranché.** `RecordingSourcesPicker.tsx:45` affiche le défaut si l'appareil mémorisé est absent, mais `startWith` envoie la valeur brute (`MeetingSetupContext.tsx:155`). Rust ne valide pas la sortie (`recording_commands.rs:240-248`) ; `get_windows_device` retombe alors sur la sortie « multimédia » par défaut (`windows.rs:239-245`), pas sur « communications » : c'est exactement le scénario du 23/10. La valeur périmée n'est jamais nettoyée. Signal : log « No matching output device found, trying default output device ».
3. **Écrans non identifiables dans le temps** — grave, probabilité moyenne. L'identifiant vient du handle Windows tronqué (`h_monitor.0 as u32`, xcap 0.9.8 vérifiée dans le dépôt) ; je suppose qu'il change après débranchement HDMI, veille ou redémarrage, et le choix mémorisé bascule sans bruit sur le principal (`screen_capture.rs:393`). « Écran n » suit l'ordre d'énumération, pas la numérotation de Paramètres > Affichage (`RecordingSourcesPicker.tsx:98`) ; les deux V277U (même nom, même résolution) donnent deux lignes indiscernables. Signal : captures du mauvais écran sans aucun avertissement.
4. **Correspondance par sous-chaîne** (`windows.rs:192`, `name.contains(base_name)`, premier trouvé) — moyenne. Avec deux sorties HDMI V277U (je suppose « V277U… » et « 2- V277U… »), la mauvaise peut être prise. Signal : sortie choisie A, enregistrement depuis B.
5. **Coût BMAD disproportionné et aucun plan de repli en direct.** PRD + stories pour un utilisateur risquent de valider le brouillon au lieu de corriger les points 1 et 2 ; si la source est mauvaise, seule issue : arrêter et relancer, réunion coupée en deux.

**À retenir pour le Président** : un choix de sortie fait au démarrage vaut-il pour cette réunion seulement, ou devient-il la règle par défaut, au risque d'écraser la détection automatique qui corrige le 23/10 ? Et comment l'utilisateur sait-il, avant « Démarrer », quel écran physique est « Écran 2 » ?
Sources citées : xcap 0.9.8 src/windows/impl_monitor.rs (https://docs.rs/crate/xcap/0.9.8/source/src/windows/impl_monitor.rs), xcap 0.9.4 (https://docs.rs/crate/xcap/0.9.4/source/src/windows/impl_monitor.rs).

### L'Architecte

**Position** : le brouillon traite le symptôme (des menus déroulants) alors que le besoin réel est de ne plus jamais perdre la voix des interlocuteurs ni l'écran partagé. Il faut d'abord rendre la capture indépendante du choix de périphérique, puis rendre ce choix visible et vérifiable.

**Arguments**
1. **On résout le mauvais problème.** Le 23/10, ce n'est pas un menu qui manquait : Teams envoie le son où il veut (ses propres réglages), et Meetily écoute une seule sortie. Un menu au démarrage transfère le même pari à l'utilisateur, au pire moment (l'appel sonne). L'approche radicale : la capture par application de Windows (process loopback), qui prend le son de Teams (ou de toute application) quelle que soit la sortie, via un périphérique virtuel (`VIRTUAL_AUDIO_DEVICE_PROCESS_LOOPBACK`), exigence build 20348, que Windows 11 dépasse (doc Microsoft). Le mode « tout sauf Meetily » (`EXCLUDE_TARGET_PROCESS_TREE`) équivaudrait à écouter toutes les sorties à la fois. Je suppose qu'il couvre les processus WebView2 du nouveau Teams : à valider par un essai d'une demi-journée avant toute story. Le menu « son de la réunion » devient un recours, pas le mécanisme principal.
2. **Le brouillon recrée l'incident en silence.** Appareil mémorisé débranché → retour au défaut sans rien dire (`RecordingSourcesPicker.tsx:43-45`, et côté backend `recording_commands.rs:250-253`). Il faut afficher l'appareil réellement enregistré (y compris ce que donne « automatique ») et signaler en clair un appareil absent. Un test de niveau avant démarrage ne vaut que pour le micro : côté système, rien ne joue avant que l'appel soit connecté. La vraie protection est pendant l'enregistrement ; l'alerte des 30 s existe (`audio_route.rs:18`) mais sans remède en un clic elle oblige à arrêter et relancer.
3. **La numérotation des écrans est fausse par construction** : ordre xcap (`RecordingSourcesPicker.tsx:96-98`, `i + 1`) sans lien avec Paramètres → Affichage ; deux V277U identiques. Il faut des miniatures (`capture_image` existe déjà, `screen_capture.rs:373-395`) ; ou « tous les écrans » par défaut puisque l'analyse d'image du résumé trie ensuite : le coût est du stockage, pas une décision au démarrage.
4. **La fenêtre de démarrage est déjà chargée** : une ligne de résumé repliée (« Micro : Intel · Son : Jabra (auto) · Écran : 2 [Modifier] ») ; en présentiel, masquer le son de la réunion.
5. **BMAD : oui, mais un PRD qui vise un résultat.** Critère de succès : « 0 réunion visio sans voix distante », mesurable avec `source_timeline`. Stories : S1 essai de la capture par application ; S2 résumé des sources affichant l'appareil réel + alerte si absent ; S3 miniatures des écrans ; S4 changement de source pendant l'enregistrement (la plus utile après S1, la plus chère).
Archives vides, aucun arbitrage contredit.

**À retenir pour le Président** : la capture par application enregistre-t-elle Teams quelle que soit la sortie, sur ce poste ? Si oui, le choix du son au démarrage devient un réglage avancé et tout le MVP se redessine. Il faut le savoir avant d'écrire la moindre story d'interface.
Sources : AUDIOCLIENT_PROCESS_LOOPBACK_PARAMS (https://learn.microsoft.com/en-us/windows/win32/api/audioclientactivationparams/ns-audioclientactivationparams-audioclient_process_loopback_params) ; PROCESS_LOOPBACK_MODE (https://learn.microsoft.com/en-us/windows/win32/api/audioclientactivationparams/ne-audioclientactivationparams-process_loopback_mode) ; miniaudio issue #484 (https://www.github.com/mackron/miniaudio/issues/484).

### L'Optimiste

**Position** : le brouillon sert à choisir des appareils, mais le code déjà écrit permet aussi de vérifier la source avant le clic. Cinq des sept questions ouvertes se règlent en réutilisant ce code. Archives vides.

**Arguments**
1. **Le test de niveau existe presque déjà.** `audio_route.rs:51-78` lit le niveau réel des sorties Windows, mais seulement pour les deux sorties par défaut. L'étendre à toutes les sorties, l'interroger toutes les 200 ms dans `RecordingSourcesPicker.tsx` : chaque appareil a sa barre, on voit que Teams joue dans le Jabra. L'erreur du 23/10 devient visible avant le démarrage, au lieu d'être signalée après 30 s. Pour les micros, je suppose qu'il faudra ouvrir un court flux d'écoute. Effort : ~1 jour. Gain : le plus fort, le seul qui prévient au lieu de rattraper.
2. **Le mode « Automatique » peut annoncer son choix.** `meeting_output()` (`audio_route.rs:103`) renvoie déjà le nom ; l'appeler à l'ouverture et afficher « Automatique (Casque Jabra) ». Effort : quelques heures.
3. **Les vignettes règlent l'identification des écrans.** Deux V277U, même nom, même résolution (`RecordingSourcesPicker.tsx:98`) ; `capture()` (`screen_capture.rs:373`) produit déjà une image par écran, `encode_frame` (`:404`) l'encode en WebP. Le choix par réunion est déjà lu côté Rust (`screen_capture.rs:220`). Effort : 0,5 à 1 jour.
4. **Un préréglage par type de réunion** (présentiel = micro Intel ; visio = Jabra + son automatique), type déjà stocké (`meeting_context.rs:39,63`) et utilisé par l'alerte (`audio_route.rs:154`). Noms d'appareils propres à chaque PC : autres postes couverts d'office ; appareil débranché → défaut (`RecordingSourcesPicker.tsx:43-45`). Effort : quelques heures.
5. **Changer de micro en cours d'enregistrement est déjà codé** (`perform_mic_hot_swap_task` / `do_mic_swap`, `recording_commands.rs:1325,1371`, utilisés seulement après déconnexion) ; les exposer : ~1 jour. Pas d'équivalent pour le son de la réunion, mais l'alerte calcule déjà la sortie de rechange (`audio_route.rs:160-164`) : un bouton « basculer sur X ». Story à part, après le MVP.

**À retenir pour le Président** : le MVP doit-il empêcher l'erreur du 23/10 avant le démarrage, ou seulement permettre de choisir à la main ? S'il doit l'empêcher, la story 1 est la barre de niveau plus le nom résolu du mode « Automatique ». Les trois listes viennent ensuite.

### L'Étranger

**Position** : le vrai problème n'est pas de pouvoir choisir ses sources. C'est de savoir, avant de démarrer, que la voix des autres est bien enregistrée. Trois listes déroulantes dont le choix est mémorisé peuvent reproduire l'incident du 23/10.

**Arguments**
1. **Mémoriser le choix va contre la sélection automatique.** Si je choisis le Jabra aujourd'hui et que demain je suis sur les haut-parleurs, la mémorisation refait le même piège. Soit l'automatique est fiable et la liste ne sert à rien, soit il ne l'est pas et c'est lui qu'il faut corriger. Tant que la règle « automatique vs dernier choix » n'est pas écrite, l'interface sera confuse.
2. **Choisir sans voir ni entendre, c'est deviner.** Une jauge de niveau en direct à côté de chaque source vaut plus que la liste : c'est le cœur du sujet.
3. **« Écran 1, 2, 3 » ne veut rien dire avec deux V277U identiques.** Sans miniature ou repère visuel, la liste est inutilisable. Pourquoi ne pas mettre « tous les écrans » par défaut et trier après ?
4. **Pourquoi choisir une seule sortie ?** Si Windows permet d'écouter toutes les sorties en même temps (supposé, non vérifié), le choix disparaît. À vérifier avant de construire une interface.
5. **Changer de source pendant l'enregistrement n'est pas un luxe** (brancher son casque après le début). Si exclu du MVP, l'alerte des 30 s doit au moins dire quoi faire.
6. **La démarche est à l'envers** : PRD et stories pour un utilisateur et trois listes, alors que le brouillon est déjà codé. En présentiel, ni le son de la réunion ni l'écran ne servent : les masquer.

**À retenir pour le Président** : après « Démarrer », comment l'utilisateur sait-il, en quelques secondes, que la voix de ses interlocuteurs est enregistrée ? Si la réponse est « il ne le sait pas », aucune liste n'empêche un nouveau 23/10.

### L'Exécutant

**Première action** : lancer le brouillon en mode dev et capturer la liste « Écran capturé » pour savoir si les deux V277U se distinguent.

**Plan 48 h**
1. Vérifier le brouillon (30 min) : `npx tsc --noEmit`, `scripts\fork-dev-env.cmd pnpm run tauri:dev`, ouvrir « Nouvelle réunion ». Je suppose que les deux V277U s'affichent à l'identique (`MonitorInfo` = id, nom, résolution, principal, `screen_capture.rs:459-465`).
2. PRD et stories BMAD courts (45 min) : `docs/specs/2026-10-28-sources-prd.md`. Objectif : aucune réunion sans le son des interlocuteurs, bon écran capturé. Hors MVP écrit : changement de source en cours, test de niveau. S1 choix au démarrage (le brouillon), S2 écrans identifiables, S3 règle de mémorisation.
3. S3 mémorisation (30 min) : fait vérifié — un choix de sortie est enregistré dans `preferred_system_device` (`MeetingSetupContext.tsx:78-80`) puis passe avant l'automatique (`recording_commands.rs:244-259`) : un choix « Realtek » une fois et le 23/10 revient. Proposition : mémoriser micro et écran, toujours reproposer « Automatique » pour le son.
4. S2 écrans identifiables (1 h 30) : ajouter x/y à `MonitorInfo`, trier par x dans `api_list_monitors`, libellé « Écran 2 — à droite · 1920×1080 ». Attention : `capture()` numérote dans l'ordre xcap (`screen_capture.rs:388-395`), garder la même numérotation pour les légendes ; l'id xcap est (supposé) un handle qui change au redémarrage → mémoriser la position.
5. Présentiel (20 min) : replier « Son de la réunion ».
6. Recette réelle (45 min) : Teams dans le Jabra en automatique ; micro Intel forcé ; écran 2 seul ; écouter l'audio et contrôler les captures ; tableau OK/KO.
7. Livraison (30 min) : commit 0.4.1-perso.7, `frontend\scripts\fork-build-installer.cmd`, installation hors réunion.

**À retenir pour le Président** : à trancher avant de coder : la mémorisation du son (étape 3), seul point qui peut ramener le 23/10. Hors MVP : changement en cours d'enregistrement et test de niveau (l'alerte des 30 s couvre le pire cas, `audio_route.rs:119-133`). Appareil mémorisé débranché remplacé par défaut côté Rust (`recording_commands.rs:250-253`) sans signal à l'écran : à ajouter aux critères de S1. Installation jamais pendant un enregistrement (vérifier `.checkpoints` et résumés en attente). `docs/conseil/` n'existe pas : aucun PV à respecter.
