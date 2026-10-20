// Meetily perso: upstream tests assert English UI text; run them with the English UI.
import i18n from '../src/i18n';

await i18n.changeLanguage('en');
