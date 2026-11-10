# DPM-3 — Diagnostiquer les supports PDF dans l’interface

## Recette manuelle

1. Enregistrer le projet en cours et fermer l’ancienne instance de DPER_MEASURE. Lancer `O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE\output\dpm3-native-supports-app\debug\dper_measure.exe`.
2. Dans **Plan de référence**, cliquer sur **Importer un plan PDF / image…** et choisir `O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE\output\dpm3-native-supports-fixtures\lineaire\supports-lineaires.pdf`. La recette utilise uniquement le plan.
3. Dans **Géométries proposées du PDF**, sélectionner la page 1 et cliquer sur **Analyser / réanalyser la page**.
4. Ouvrir **Vues et associations du plan PDF**. Dans **1. Définir une vue**, cliquer sur **Préparer / recharger la vue**, nommer la vue, cocher la cote de 18 mm et les traits ci-dessous. Cliquer sur **Enregistrer la vue explicite**.
5. Dans **2. Choisir une association**, choisir cette vue et cette cote, puis **Préparer le diagnostic des supports**. Choisir les deux traits, renseigner la justification et cliquer sur **Diagnostiquer les supports**. Ouvrir **Sources et limites du diagnostic** pour consulter les identifiants, coordonnées et données sources.
6. Choisir ensuite le trait oblique à la place d’un des supports. Vérifier que l’ancien résultat disparaît et que le diagnostic refuse les supports non parallèles. Le nombre de mesures et l’étalonnage restent identiques.
7. Refaire les étapes 2 à 5 avec `O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE\output\dpm3-native-supports-fixtures\angulaire\supports-angulaires.pdf` et sa cote de 123°. Vérifier les deux angles possibles, leur intersection et l’absence de secteur sélectionné.

| Plan | Traits à choisir, en points PDF | Résultat attendu |
| --- | --- | --- |
| Linéaire | (40, 80)–(160, 80) et (60, 120)–(140, 120) | Séparation 40 pt, recouvrement 80 pt |
| Linéaire, refus | Un des deux traits précédents et (200, 80)–(240, 100) | Refus des supports non parallèles |
| Angulaire | (60, 100)–(180, 100) et (80, 60)–(160, 140) | Intersection (120, 100) pt, angles possibles 45° et 135° |

Les cotes de 18 mm et 123° viennent du plan. Le diagnostic calcule séparément la géométrie des deux traits choisis. Le résultat reste une proposition documentaire à vérifier. La reconnaissance des vues et des rôles de traits ainsi que la persistance d’une association linéaire ou angulaire restent à traiter.

## Reproduire les plans de test

Depuis `O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE`, utiliser des dossiers de sortie neufs.

```powershell
python scripts/verify_linear_drawing_supports_mcp.py output/dpm3-linear-recipe
python scripts/verify_angular_drawing_supports_mcp.py output/dpm3-angular-recipe
```

## Vérifications exécutées

- `cargo test --locked --bin dper_measure inspection_plan_ui::tests::` — 17 réussis, 0 échec, 433 filtrés. Deux nouveaux tests exercent les clics et la saisie egui. Ils contrôlent les valeurs affichées, l’effacement du résultat après édition de la justification et l’absence de mutation du plan, de la calibration et des mesures. La première exécution a mis en évidence des erreurs de préparation dans le code des tests et un échec de remplacement de fichier dans le dossier temporaire par défaut. Les tests ont été corrigés et relancés avec `TEMP` et `TMP` sous `output/`.
- Recettes sur serveur MCP dédié, port 18771, avec extraction Poppler — 9 contrôles linéaires et 13 angulaires réussis. L’état de la session a été comparé et l’hôte a été arrêté normalement. Preuves locales dans `output/dpm3-native-supports-mcp-20261110-202532-326/`.
- `cargo build --locked --bin dper_measure` — compilation réussie, 40 avertissements de code inutilisé. La recette ci-dessus couvre la fenêtre Windows et les données sont synthétiques.
