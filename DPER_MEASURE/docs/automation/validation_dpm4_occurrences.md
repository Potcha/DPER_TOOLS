# DPM-4 · Affecter les occurrences d’une cote répétée

Une cote de rayon ou de diamètre répétée entre 2 et 64 fois peut être répartie entre autant de cercles proposés d’une même vue PDF. L’ordre du choix fixe les numéros. Chaque affectation conserve les identifiants des candidats et des objets PDF, leurs empreintes, la page, la vue et la justification.

L’interface et MCP utilisent la même opération. Le groupe reste une proposition documentaire. Le contrôle physique reste suspendu jusqu’à la prise en charge des observations par occurrence et des références géométriques. Le projet écrit au schéma 19 et les projets au schéma 18 restent lisibles. La réouverture conserve la trace et demande de renouveler le choix dans la session courante.

## Reproduire la recette

Depuis le dossier DPER_MEASURE, avec Cargo, Python et les outils PDF utilisés par la recette d’inspection :

```powershell
pwsh -NoProfile -File scripts/test_inspection_workflow.ps1 -OutputDirectory output/validation_dpm4_occurrences_revue
```

Choisir un dossier de sortie inédit. La recette génère le plan `repeated_requirement.pdf` dans le sous-dossier `mcp`, ainsi que le projet et le rapport synthétiques.

## Vérifier dans l’interface

1. Importer le PDF synthétique comme plan de référence. Dans **Géométries proposées du PDF**, analyser la page 1. Le PDF contient cinq cercles répartis sur quatre centres.
2. Dans **Vues et associations du plan PDF**, préparer une vue avec la cote `4×Ø4.0 ±0.1 mm` et les cinq cercles. Enregistrer la vue.
3. Sélectionner cette vue et cette cote, puis cliquer sur **Préparer / recharger les associations**. Choisir quatre cercles de centres distincts dans l’ordre souhaité et saisir une justification.
4. Cliquer sur **Enregistrer les occurrences choisies**. Le volet **Répétitions et relations du plan** affiche l’ordre, la provenance et le statut. Le compteur de mesures conserve sa valeur. Si les deux cercles concentriques sont sélectionnés, l’enregistrement est refusé.
5. Enregistrer et rouvrir le projet. Les quatre choix restent lisibles et leur état est suspendu. Le PDF de recette présente les quatre associations dans la section de traçabilité.

Via MCP, obtenir les identifiants avec `get_drawing_binding_candidates`, puis appeler `assign_drawing_occurrences` avec la révision courante, les identifiants de la vue et de la cote, la liste ordonnée des candidats et la justification. `get_inspection_state` fournit les occurrences proposées et l’état de leur affectation.

## Résultats

- 323 tests Rust réussis, zéro échec. L’hôte de test, ignoré par défaut, a ensuite été exécuté séparément avec succès. Compilation debug réussie avec les avertissements existants.
- Clics egui sur les candidats, numérotation dans l’ordre choisi, puis suspension visible après remplacement du PDF. Le transport MCP a exercé le cas positif et les refus pour liste incomplète, objet répété, centres concentriques et révision périmée. Le refus de mesurer la cote entière avec un seul cercle reste couvert.
- Les quatre affectations figurent dans le PDF de recette, vérifié par extraction du texte. Le projet au schéma 19 conserve les quatre traces après réouverture et le remplacement de la vue suspend aussi le groupe.
- Les contrôles existants de vision, dégagement, attache, unité et pointe passent sur données synthétiques. L’hôte dédié au port 18766 a été arrêté et le profil utilisateur est resté identique. La recette ne qualifie aucune mesure sur pièce.
