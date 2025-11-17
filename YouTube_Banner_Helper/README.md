# YouTube_Banner_Helper

## FR

Outil ultra leger pour verifier que vos visuels respectent les gabarits YouTube
(banniere + avatar). Il suffit d'ouvrir le fichier HTML local et de charger votre image.

### Objectif
- Previsualiser l'image finale sur desktop, TV et mobile.
- Identifier les zones de securite (texte/logo) et la zone de l'avatar.
- Exporter rapidement une version ajustee sans passer par un outil lourd.

### Prerequis
- Navigateur moderne (Chrome, Edge, Firefox, etc.).

### Utilisation
1. Ouvrir `yt-channel-customizer.html` dans votre navigateur (double-clic ou `Ctrl+O`).
2. Cliquer sur le bouton d'import et selectionner votre bannere.
3. Ajuster zoom/position jusqu'a ce que la zone securisee contienne l'essentiel.
4. Telecharger/exporter si necessaire (le navigateur fournit le PNG resultat).

### Conseils
- Prevoir une image 2560x1440 minimum pour couvrir tous les formats.
- Garder les textes/branding dans la zone centrale 1546x423.
- Tester fonds clairs/sombres pour anticiper le mode sombre YouTube.

Branches recommandees : `feature/youtube_banner_helper-<ticket>` depuis `dev`.

### Aller plus loin
- Ajouter vos propres reperes en dupliquant le fichier HTML.
- Reporter les idees/bugs en creant une issue etiquetee `YouTube_Banner_Helper`.

---

## EN

Tiny helper to preview whether your banner artwork matches YouTube's safe areas
(banner + profile picture). Just open the local HTML file and load your image.

### Goals
- Preview final look on desktop, TV, and mobile breakpoints.
- Highlight safe zones for text/logo and the circular avatar crop.
- Quickly export an adjusted version without Photoshop.

### Requirements
- Any modern browser (Chrome, Edge, Firefox, ...).

### Usage
1. Open `yt-channel-customizer.html` in your browser (double-click or `Ctrl+O`).
2. Click import and pick your banner.
3. Adjust zoom/position until the safe area contains all critical elements.
4. Export/download if needed (browser provides the resulting PNG).

### Tips
- Use at least 2560x1440 artwork.
- Keep branding within the centered 1546x423 safe area.
- Try light/dark backgrounds to anticipate YouTube dark mode.

Recommended branches: `feature/youtube_banner_helper-<ticket>` from `dev`.

### Going further
- Duplicate the HTML file to add custom guides.
- File bugs/ideas via an issue labeled `YouTube_Banner_Helper`.
