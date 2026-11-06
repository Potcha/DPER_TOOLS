# DPER_MEASURE — interface d’origine récupérée

Le code graphique et les interactions de la sauvegarde
`DPER_MEASURE_avant_sync_20261105_194955/fichiers_locaux.zip` sont récupérés dans
DPM-42. Ce lot complète la première adaptation du bandeau livrée par DPM-29 / PR #33.
La sauvegarde originale reste intacte. Les écrans n’ont pas été redessinés à
partir des captures.

Sont rétablis : les neuf onglets et leurs PNG, ruban et sous-menus, timeline
fonctionnelle, messages, panneau Mesures/Annotations, cartouches cyan/orange,
poignées, flèches, constructions violettes, variantes géométriques, motifs,
multisélection et historique. Longueur connue précède Valider puis Annuler.

Les PNG des onglets sont chargés par `include_bytes!` dans
`src/recovered_chrome_ui.rs` depuis `assets/ui/ribbon_tabs/atlas_onglets.png`.
Le vrai bouton egui est dessiné par `ui_widgets::navigation_tab_button`.
Les images ne remplacent pas les contrôles interactifs.

Les composants du ruban sont ceux écrits pour DPER_MEASURE ; aucun code de
SolidCraft n’est inclus. Les services récents de contrôle et leurs protections
sont conservés : une construction, une copie ou un contour affiché ne devient
pas automatiquement une preuve physique.

La provenance détaillée, les résultats exacts et la procédure de réception
figurent dans [validation_dpm42.md](automation/validation_dpm42.md).
Les essais de DPM-29 restent documentés comme historique dans
[validation_dpm29.md](automation/validation_dpm29.md).

Pour essayer l’interface dans le même dossier :

```powershell
Set-Location 'O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE'
cargo run --locked
```

Le mode explicite `--ui-review NOUVEAU_DOSSIER` capture l’application native sur
une image synthétique avec mesures et constructions, sans caméra ni serveur MCP.
Il ne charge pas les réglages utilisateur. Les sorties de recette restent sous
`output/`, exclues de Git.
