# DPM-3 — Géométries PDF et associations explicites

Cette tranche fournit l'extraction vectorielle locale, le moteur d'associations candidates et deux commandes CLI. Elle utilise les bases DPM-2, DPM-6, DPM-10 et DPM-14 déjà intégrées. Le parcours MCP et sa revue dans l'interface font l'objet de la tranche d'intégration DPM-18.

## Utilisation

Les exécutables Poppler `pdftotext` et `pdftocairo` doivent être accessibles dans le `PATH`. Depuis `DPER_MEASURE/` :

```powershell
cargo build --locked
.\target\debug\dper_measure.exe --cli drawing-plan plan.pdf plan-controle.json
.\target\debug\dper_measure.exe --cli drawing-geometry plan.pdf 1 geometries-page-1.json
```

Chaque sortie doit être un nouveau fichier. Les commandes préservent le PDF source et refusent d'écraser une sortie existante. L'extraction vectorielle accepte les pages 1 à 20 et limite le SVG à 16 Mio et son nombre de primitives/segments. L'empreinte du contenu est vérifiée avant et après extraction.

`drawing-plan` extrait des caractéristiques textuelles conservatrices avec leurs unités, tolérances, positions et ambiguïtés. `drawing-geometry` produit des primitives candidates avec leurs styles, transformations et provenance. Les coordonnées sont des points PDF, origine en haut à gauche, x vers la droite, y vers le bas. Elles ne constituent jamais une calibration physique.

## Associations proposées par la bibliothèque

`dper_binding::define_view` définit un périmètre explicite à partir des identifiants du plan et du catalogue d'une même source/page. `binding_candidates` propose des associations entre une cote de rayon/diamètre et des cercles complets appartenant à cette vue. La proximité classe les alternatives sans comparer leur rayon au nominal pour choisir un résultat.

`select_binding` conserve une hypothèse justifiée et non revue. `resolve_binding` contrôle la cohérence source, caractéristique, vue et primitive avant de retourner une géométrie de recherche. Les appelants doivent encore vérifier la source disque courante, la revue humaine, le recalage et les observations réelles ; résoudre une association ne valide aucune mesure. La revue n'est pas restaurée depuis du JSON.

Les hypothèses concentriques ou symétriques restent distinctes. Profondeurs, types non pris en charge, unités angulaires, supports circulaires insuffisants et incohérences d'identifiants empêchent la résolution. Une unité absente ou exprimée en pouces n'empêche pas de proposer une association géométrique : son interprétation et sa conversion doivent encore être établies par le parcours de contrôle avant toute décision physique. Les clips/masques et commandes vectorielles non résolus restent signalés dans le catalogue. Un catalogue vide ou partiel ne prouve pas l'absence d'une caractéristique.

## Vérification reproductible sur les données livrées

Les entrées synthétiques et leurs attentes indépendantes sont dans `tests/fixtures/drawing_geometry/` : cercle avec renvoi, deux cercles concentriques et deux vues symétriques. Le vérificateur les utilise par défaut, sans dépendre de sorties locales antérieures.

```powershell
cargo test --locked dper_pdf_geometry
cargo test --locked dper_binding
python scripts/verify_vector_fixtures.py --output-dir output/automation-validation/dpm3-nouvel-essai
```

L'exécutable peut être précisé avec `--executable`. Le dossier de résultats doit être nouveau. `result.json` conserve les comptes, écarts aux centres/rayons attendus, empreintes et limites des contrôles. Le seuil par défaut est de 0,15 point PDF ; il ne définit aucune incertitude physique.

Cette tranche ne réalise pas la reconnaissance sémantique générale des vues ou renvois, l'appariement automatique avec une photo, la mesure d'extrémités, la correction de perspective ni un cercle de passage entre plusieurs butées. Les constructions du plan servent à localiser une recherche ; seuls les pixels d'une image calibrée peuvent établir une dimension observée.
