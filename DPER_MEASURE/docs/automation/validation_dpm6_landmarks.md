# DPM-6 — Conservation des repères de recalage

Validation logicielle du 31 octobre 2026 sur une sélection issue de `dev`, base `bdc82c9181e7c91731f753a9e5cb30cdd7c6b594`. Ce lot complète le critère de traçabilité restant de DPM-6 ; les PR précédentes avaient livré le calcul robuste, ses indices et ses statistiques, mais les coordonnées des correspondances n'étaient pas conservées.

## Correction

Le recalage conserve maintenant la liste originale ordonnée des couples plan/image, les indices retenus, les résidus de tous les points et le seuil utilisé. Les résidus individuels suivent le même ordre que les correspondances, y compris pour les points rejetés. RMS et maximum restent calculés sur les seuls points retenus. L'algorithme de sélection et l'étalonnage physique ne changent pas.

Le résultat est accessible par MCP, sauvegardé dans le projet et conservé dans le manifeste du rapport. Le panneau natif d'inspection permet d'en consulter les coordonnées, statuts et résidus. L'écriture de projet passe au schéma 15 ; les schémas anciens restent acceptés et les versions futures restent refusées. Un ancien recalage sans coordonnées reste lisible avec des listes vides et un seuil inconnu : aucun repère n'est inventé et le panneau affiche son caractère historique.

## Vérifications exécutées

- `cargo test --locked` : **128 tests réussis, zéro échec**, sur le contenu sélectionné.
- `cargo build --locked` : réussi, avec 18 avertissements de compilation documentés dans les traces.
- Trois nouveaux tests du moteur : conservation de l'ordre et de l'aberrant, aller-retour JSON des nouveaux champs et lecture d'un ancien rapport sans coordonnées.
- Test de persistance de session enrichi : coordonnées, indices, résidus et seuil conservés après désérialisation ; la preuve d'inspection importée reste historique.
- Contrôle des versions : ancien schéma 14 conservé comme 14, schéma courant 15 accepté, schéma futur refusé par les deux désérialisations.

Le scénario applicatif `scripts/run_inspection_smoke.ps1` a ensuite utilisé une instance dédiée sur **18765**, avec des fichiers synthétiques neufs. Il a exercé 18 opérations existantes, dont le nouveau contrôle de traçabilité :

| Cas | Résultat observé |
|---|---|
| Cinq correspondances fournies | Les cinq couples sont conservés dans l'ordre original. |
| Sélection robuste | Quatre repères retenus, le cinquième rejeté. |
| Résidus individuels | Cinq valeurs conservées, comparées à la transformation indépendante attendue ; le point rejeté dépasse 200 px pour un seuil de 0,1 px. |
| Sauvegarde et rechargement | Couples, indices, seuil et cinq résidus conservés ; erreurs de comparaison inférieures à 1e-9 px. |
| Rapport exporté | Projet schéma 15 et manifeste conservant la même trace de recalage. |
| Calibration indépendante | Profil complet inchangé avant/après recalage et rechargement. |
| Isolation | Profil utilisateur inchangé ; le processus de test possédé a été arrêté. |

Les scénarios existants restent exercés : observations sur pixels, constructions exclues des valeurs, révisions périmées et fausses preuves refusées, suspension après édition/rechargement, dépendances et ordre de restauration. Aucune recette sur pièce réelle n'est revendiquée.

La preuve portable et les empreintes du code sont dans [evidence/dpm6_landmarks.json](evidence/dpm6_landmarks.json). Les RPC, projets, PDF et manifestes bruts restent locaux dans le dossier de livraison `output/delivery/DPM-6-landmarks/`. La revue du nouveau panneau est une lecture du code et une compilation ; aucune recette visuelle complète de l'interface n'est annoncée. DPM-22 conserve ce parcours natif complet.

Ce lot est soumis à revue, sans fusion automatique. Le changement Jira de DPM-6 à « Terminé » attend la validation et la fusion de sa PR. La branche et l'index utilisateur restent conservés, sans nouveau worktree et sans publication de données industrielles.
