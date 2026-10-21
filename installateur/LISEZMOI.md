# Installer Meetily perso sur un autre poste

Le fichier `Meetily-perso-<version>-setup.exe` de ce dossier installe la version perso de Meetily sur n'importe quel PC Windows 10 ou 11 (64 bits). Il ne contient aucune clé ni aucune réunion : chaque poste démarre vide.

Pour refaire l'installateur après une modification du code : `frontend\scripts\fork-build-installer.cmd`. Il remplace le fichier de ce dossier par la nouvelle version.

## Installation

1. Copier le `.exe` sur le poste (clé USB, OneDrive…) et le lancer.
2. L'installateur n'est pas signé : Windows affiche « Windows a protégé votre ordinateur ». Cliquer sur « Informations complémentaires », puis « Exécuter quand même ».
3. Suivre l'assistant. Meetily s'installe pour l'utilisateur courant, sans droits administrateur.

Si une version officielle de Meetily est déjà installée sur ce poste, la version perso la remplace et reprend ses réunions. Sauvegarder d'abord `%APPDATA%\com.meetily.ai\meeting_minutes.sqlite`.

## Réglages à refaire sur le nouveau poste

| Réglage | Où | Quoi |
|---|---|---|
| Clé OpenRouter | Réglages → Cloud & IA | La clé RISS, comme sur le premier poste |
| Transcription cloud | Réglages → Transcription → OpenRouter (cloud) | Choisir le modèle (elevenlabs/scribe-v2 par défaut) |
| Langue | Réglages → Général | Français |
| Notion | Réglages → Notion | Coller la clé de l'intégration « Meetily ». La page « Meetily » est déjà connectée : la choisir, puis « Créer la base ». Meetily retrouve la base « Réunions Meetily » existante et y ajoute les réunions de ce poste, sans créer de doublon. |
| Tags | Réglages → Tags | Les tags sont propres à chaque poste ; les recréer avec les mêmes noms pour garder un classement cohérent dans Notion. |

Les modèles locaux (Parakeet pour le repli hors connexion) se téléchargent au premier lancement, comme dans la version officielle.
