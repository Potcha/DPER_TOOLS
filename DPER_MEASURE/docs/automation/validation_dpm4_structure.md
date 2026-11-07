# DPM-4 — Déclarations de répétition et références à résoudre

Une cote telle que `4× Ø4 ±0,1 mm` déclare quatre occurrences à contrôler. Le plan conserve la déclaration et sa provenance. Le groupe indique séparément le nombre attendu et les occurrences identifiées. La confirmation du texte ou l'association d'une mesure unique laisse le groupe incomplet.

Le panneau **Répétitions et relations du plan** expose ces informations. `get_inspection_state` et `analyze_drawing` renvoient `drawing_structure`, avec les groupes et les relations documentaires. Le PDF de contrôle restitue la structure du plan dans les preuves. Elle est recalculée à partir des champs existants après réouverture, avec le même schéma de projet.

Chaque relation déclarée cite la caractéristique dépendante, sa source, son type, sa cible et sa justification. Une cible unique permet de consulter sa provenance, avec un état de revue distinct. Une cible absente, dupliquée ou une déclaration invalide reste suspendue. Le texte d'un contrôle géométrique sans référence interprétée reste disponible pour revue.

Le contrôle assisté suspend les nouvelles observations rattachées à ces exigences et réévalue les preuves existantes comme indéterminées. Le comparateur scalaire du panneau de référence suspend aussi les déclarations de répétition qu'il reconnaît dans le texte conservé. La mesure manuelle d'une occurrence reste possible. Les cotes simples et les groupes de résultats déjà enregistrés conservent leur propre parcours.

Le même garde couvre les limites `MIN/MAX`, y compris les anciens projets dont la lecture avait omis le nombre d'occurrences. Une réinterprétation du type conserve la suspension lorsque le texte source porte un symbole géométrique reconnu.

## Recette de revue

1. Importer un PDF de test avec `4×Ø4.0 ±0.1 mm` et une cote simple distincte.
2. Dans le panneau du plan, vérifier le groupe de quatre occurrences attendues et sa source. Confirmer la lecture du texte et vérifier que le motif de suspension reste visible.
3. Via MCP, vérifier `drawing_structure.groups`, puis tenter d'enregistrer la cote répétée avec `inspect_feature`. L'opération doit refuser avec un motif sur les occurrences, sans modifier les mesures ni la révision.
4. Comparer une cote simple à une mesure indépendante dans le parcours habituel pour vérifier que la suspension ne s'étend pas au reste du plan.

## Périmètre de cette livraison

Cette tranche livre la représentation et la suspension des exigences non résolues. L'individualisation et la revue des membres, la résolution des références géométriques et leur raccordement aux preuves restent à livrer sous DPM-4. Leur extraction automatique des symboles et des vues dépend aussi des capacités de DPM-3.

## Résultats des tests

La recette `scripts/test_inspection_workflow.ps1` a réussi avec **322 tests Rust, zéro échec**, une compilation debug et l'hôte de test MCP lancé séparément. Un PDF synthétique a fourni une déclaration de quatre occurrences et les identifiants de mots de sa source. L'enregistrement d'une mesure unique a été refusé sans modification des mesures ni de la révision. La même recette a vérifié les huit passages de DPM-43 et l'export groupé. Après le dernier ajustement de compatibilité des anciens projets MIN/MAX, les cinq tests ciblés `cargo test --locked drawing_structure` ont réussi sur le contenu final.
