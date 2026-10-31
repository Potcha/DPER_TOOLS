# DPM-19 — Dépendances et conservation prudente des preuves

Le lot est fondé sur `85d4cf765d04c5eefcad71bd56b0e7a1991086e3` (PR DPM-18 #15). La validation du 31 octobre 2026 porte sur cette sélection complète, dans le dossier de livraison de l'outil, sans nouveau worktree ni modification de la branche et de l'index utilisateur. La PR #15 doit être intégrée avant ce lot.

## Résultat

Les constructions et mesures peuvent être reliées par `source_measurement_ids`. Le graphe impose des identifiants uniques, des parents présents, une capture commune et l'absence de cycles. Une relation explicite est une provenance, sans équation ni validation implicite. Les anciens projets sans relations restent lisibles ; aucune relation n'est inférée de leur dessin.

Les générations de nœuds vivent hors des clones utilisés par undo/redo. Toute édition matérielle invalide les observations liées et les descendants. Revenir aux anciens points ne rétablit pas la preuve. Déplacer ou renommer seulement un libellé n'invalide pas la géométrie. Calibration, protocole, remplacement du plan/recalage et associations sont raccordés ; l'index est conservé quand le plan est remplacé. Les compteurs épuisés échouent prudemment.

Une source PDF modifiée suspend aussi les descendants de ses observations lors de la consultation MCP ou du rafraîchissement du panneau. Les parents vision doivent posséder une observation actuelle avant de servir de source à un nouvel enfant. L'absence de qualification physique continue à suspendre la conformité ; elle n'est pas transformée en validation par le graphe.

Un descendant périmé conserve sa géométrie pour inspection, mais expose `dependency_stale: true`, des composantes numériques vides et « À recalculer · dépendances modifiées ». Les comparaisons, CSV/XLSX et PDF/PPTX ne lui accordent plus de valeur courante ni d'ancien verdict. Une racine manuelle indépendante reste inchangée.

La suppression d'un parent seul est refusée tant que des descendants subsistent. Un lot est supprimé dans l'ordre enfants/parents ; les annulations restaurent les parents avant leurs enfants. Les projets sauvegardés restent donc rechargeables. Au chargement, les dépendances sont vérifiées et les dérivés deviennent périmés, même si le JSON affirme le contraire. Le schéma **14** conserve la lecture historique et refuse toute version future dans tous les chemins de désérialisation de `Session`.

## Vérification exécutée

Sur le contenu sélectionné :

```powershell
cargo test --locked
cargo build --locked
```

**125 tests réussis, zéro échec ; compilation réussie avec 18 avertissements.** Les tests couvrent notamment les graphes sur plusieurs niveaux, le frère indépendant, les cycles/parents absents/identifiants ambigus/captures différentes, les tombstones, l'import, les générations d'observation, les versions futures et les sorties numériques/PDF/PPTX. Le support de test de rapports crée son dossier de sortie, y compris avec une cible de compilation externe.

Le scénario MCP lancé via `scripts/run_inspection_smoke.ps1` a exercé **18 opérations** (les onze opérations d'inspection et sept opérations existantes de contexte, édition, historique et projets) dans une instance dédiée sur **18765**, avec des fichiers synthétiques neufs.

| Cas exécuté dans l'application | Résultat |
|---|---|
| Construction → cercle observé → distance dérivée | Propagation de l'état périmé sur deux niveaux |
| Racine manuelle indépendante | Géométrie et composantes conservées |
| Édition puis undo/redo | Points restaurés, ancienne preuve toujours suspendue |
| Parent supprimé seul | Refus sans mutation de contexte |
| Suppression des descendants en lot, puis deux undo | Parent restauré avant son enfant, dérivés toujours périmés |
| Projet cyclique ou schéma 999 | Refus sans changement de capture, de révision ni de mesures |
| JSON affirmant une fraîcheur restaurée | Tous les dérivés importés restent périmés |
| Parent vision importé | Refus de créer un nouvel enfant sans nouvelle observation |
| PDF modifié avec la même longueur de fichier | Observation liée et descendant suspendus |
| Rapport et manifeste | PDF de cinq pages lisible ; avertissements de recalcul présents |
| Isolation | Profil de calibration inchangé et processus de test arrêté |

Les contrôles du lot précédent restent exercés : séparation nominal 4,000 mm / diamètre attendu 4,200 mm / observé 4,207240 mm, capture/révision incorrectes refusées, faux score sans contour refusé, fit robuste conservé par la mesure native. Le seuil synthétique de 0,020 mm ne constitue pas une incertitude physique.

`pdfinfo` et `pdftotext -layout` ont lu le rapport sans erreur de syntaxe. La page 5 a été rendue puis inspectée : les deux résultats périmés affichent le recalcul requis, la racine indépendante garde 1,050 mm, sans débordement constaté. Le moteur de rendu Poppler signale toujours une substitution de police `Symbol` sur ce poste ; cette réserve est conservée.

Les preuves portables et les empreintes du code sont dans [evidence/dpm19.json](evidence/dpm19.json). Les traces RPC, projets et PDF bruts restent locaux sous `output/delivery/DPM-19/` ; aucune donnée de pièce réelle n'est publiée.

## Domaine restant

Les producteurs de motifs doivent déclarer leurs liens via le même contrat ; le code de motifs encore présent dans l'arbre local mixte n'est pas incorporé à cette PR. Aucune recalculation géométrique automatique ni migration supposée des dépendances anciennes n'est fournie. Une nouvelle inspection ou un nouveau nœud est requis pour remplacer un résultat périmé. Le parcours natif complet reste dans DPM-22. La qualification physique et le passage entre quatre butées demandent toujours leurs travaux dédiés.
