# DPM-18 — Validation de l'inspection assistée, phase 1

Validation réalisée le 31 octobre 2026 sur une sélection assemblée depuis `dev` au commit `4d4c57efce1f049e1ace0cfecfcd344e39d754c5`. Les sources, les scripts et leurs empreintes sont répertoriés dans [la preuve portable](evidence/dpm18.json). La branche de travail et l'index utilisateur sont conservés ; cette livraison ne contient que le lot sélectionné dans `DPER_MEASURE/`.

## Comportement livré

Les onze opérations MCP d'analyse, de catalogue vectoriel, de vue explicite, d'association, de recalage et d'observation utilisent la session propriétaire de l'application. Les mutations vérifient la révision et, lorsqu'elles visent l'image, la capture. Le calcul d'extraction PDF vérifie de nouveau la révision à l'installation. Les garde-fous d'édition consultent les actions UI en cours ; ce dernier cas a été revu dans le code, sans simulation d'un geste à la souris.

L'observation recalcule les contours depuis les pixels réels. Un cercle natif enregistré conserve exactement le centre et le rayon de l'ajustement robuste. Les enregistrements portent un groupe, la caractéristique du plan, les échantillons observés et leurs limites. L'UI permet la revue et la sélection pour correction manuelle ; elle ne fournit pas encore toutes les commandes de lancement du parcours.

Le schéma de projet 13 conserve l'inspection. Une preuve rechargée reste historique et une association rechargée perd sa revue humaine. Les constructions restent visibles mais sont exclues des composantes numériques et de la conformité. La comparaison manuelle existante et ses bandes de garde sont conservées. Les résultats `vision` ne reçoivent pas automatiquement l'incertitude de calibration ni un verdict de conformité.

## Contrôles exécutés

Depuis le dossier de l'outil, sur les sources sélectionnées :

```powershell
cargo test --locked
cargo build --locked
```

**115 tests Rust réussis, zéro échec.** Compilation réussie, avec 18 avertissements de compilation, principalement des éléments non utilisés. Les contrôles couvrent aussi les modules préexistants de dessin, recalage et vision, les comparaisons manuelles avec incertitude, la persistance et les exports. Quatre régressions ciblent l'exclusion des constructions et la suspension des verdicts `vision` dans la comparaison, CSV/XLSX, PDF et PPTX.

Le scénario `scripts/run_inspection_smoke.ps1` a ensuite lancé un exécutable issu de cette sélection sur le port **18765**, avec des fichiers synthétiques neufs. Le port a été attribué au processus de test avant tout appel MCP. Le profil utilisateur de calibration a la même empreinte avant et après ; le processus possédé par le test a été arrêté. Aucune pièce ni aucun plan industriel n'a été utilisé.

| Contrôle MCP/application | Résultat observé |
|---|---|
| Catalogue des opérations | Onze opérations présentes et exercées |
| Nominal indépendant de l'observation | Nominal du PDF 4,000 mm ; diamètre synthétique attendu 4,200 mm ; diamètre observé 4,207240 mm |
| Recherche depuis une association PDF | 4,207241 mm, sans coordonnées de géométrie dans `inspect_bound_feature` |
| Précision logicielle synthétique | Écart absolu 0,007241 mm, inférieur au seuil d'essai de 0,020 mm |
| Transfert vers la mesure native | Centre/rayon identiques au fit robuste ; écart numérique de diamètre inférieur à 1e-9 mm |
| Appel périmé ou mauvaise capture | Refus, sans mutation incohérente de révision, mesures ou enregistrements |
| Faux score/preuve client sur une zone sans contour | Refus : les données fournies ne remplacent pas les pixels |
| Construction explicite | Composantes de mesure vides |
| Modification manuelle | Preuve liée invalidée |
| Rechargement | Preuves historiques ; absence de validation actuelle automatique |
| Association non revue | Géométrie `measured`, conformité `indeterminate` |
| Recalage sans la bonne vue | Refus |
| PDF source modifié à longueur de fichier identique | Refus d'inspecter ; motif `drawing_source_changed` à la relecture |
| Export applicatif | PDF et manifeste compagnon produits, avec preuves et limites |

Le seuil de 0,020 mm est une tolérance de ce test synthétique. Il ne constitue ni une incertitude de mesure ni une qualification physique.

Le PDF lié contient quatre pages A4. `pdfinfo` et `pdftotext -layout` ont pu le lire sans erreur de syntaxe. Les pages 2 (preuves/limites) et 4 (image, groupe et tableau) ont été rendues puis inspectées visuellement : pas de débordement constaté ; le verdict reste suspendu. Le rendu Poppler signale toutefois `No display font for 'Symbol'` sur ce poste. Cette réserve de substitution de police est distincte de la lecture syntaxique réussie. Le tracé PDF de signature a été corrigé : il débute maintenant par un déplacement valide avant le segment.

Un premier essai avait révélé cette erreur de tracé ; les traces d'échec restent conservées localement. Les résultats ci-dessus proviennent du nouvel essai réussi après correction. Les sorties brutes et captures de ce test restent dans `output/delivery/DPM-18/` ; seul le résumé portable et les empreintes sont livrés.

## Reproduire après intégration

Prérequis : chaîne Rust du projet, Windows/PowerShell, Python avec Pillow et les outils Poppler (`pdftotext`, `pdfinfo`, `pdftoppm`) accessibles. Depuis `DPER_MEASURE/` :

```powershell
pwsh -NoProfile -File scripts/run_inspection_smoke.ps1
```

Le script compile puis crée un dossier neuf sous `output/automation-validation/`. Il refuse le port utilisateur 8765 et les dossiers de sortie existants. Pour réutiliser une compilation contrôlée, `-SkipBuild -ExecutablePath <chemin-absolu>` désigne explicitement l'exécutable à tester. Les données produites sont synthétiques ; le script ne doit pas être redirigé vers une session utilisateur ouverte.

## Limites et tickets restants

- **DPM-18** : cette phase d'orchestration bornée est soumise à revue. Elle n'est pas encore fusionnée ni déclarée terminée par ce compte rendu.
- **DPM-19** : la persistance, les groupes, les constructions et l'invalidation directe sont raccordés ; le graphe général de dépendances, ses descendants et tous les cas de motifs restent à compléter.
- **DPM-22** : le panneau de revue et les superpositions sont livrés avec le socle ; les commandes natives complètes de préparation/recalage/inspection et leur recette visuelle restent à faire.
- **DPM-3** : les vues et associations sont explicites ; la reconnaissance sémantique générale et l'appariement automatique plan/photo restent à développer.
- Le cercle tangent à quatre butées, les extrémités réelles, les profondeurs, les faces cachées et la perspective demandent des méthodes supplémentaires. Le présent contour circulaire ne mesure pas le passage entre butées.
- Aucune qualification physique, incertitude totale par mesurande ou recette sur pièce réelle n'a été effectuée. Aucun verdict d'acceptation industrielle n'est revendiqué.
