# DPM-15 — budget scalaire avec corrélations explicites

## Portée et critères

Le nouvel outil MCP `calculate_correlated_uncertainty_budget` calcule `u_c² = somme(i,j) (c_i u_i) rho_ij (c_j u_j)`, puis `U = k u_c`. Il conserve les signes des sensibilités, la matrice complète dans l'ordre des composantes, les sources et la justification du modèle de corrélation. Chaque coefficient a pour unité **unité de sortie / unité d'entrée**. Les sorties sont explicitement `millimeter`, `degree` ou `pixel`, sans conversion implicite.

Le budget indépendant `calculate_uncertainty_budget` reste inchangé et continue de refuser les champs de corrélation. Le nouvel outil ne lit pas la session, n'applique pas U à une mesure, ne qualifie pas les sources et ne décide aucune conformité. `status=calculation_only`, `applied_to_measurement=false`, `source_qualification_verified=false`, `conformity_evaluated=false`.

Critères de recette : résultats analytiques à une tolérance relative de `1e-12`, signes et sources conservés, refus des matrices invalides ou numériquement indécidables, et contexte de session inchangé après **chaque** appel.

## Protocole manuel dès la livraison

1. Fermer l'ancienne instance et lancer :

   ```powershell
   & 'O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE\output\dpm15-correlated-app\debug\dper_measure.exe'
   ```

   Aucune image ni plan PDF n'est nécessaire. L'installateur existant ne contient pas cette évolution.
2. Demander au chat : **« Exécute la recette DPM-15 des budgets corrélés via MCP »**. Le chat relève `get_measurement_context` et `get_inspection_state`, puis vérifie les calculs et refus ci-dessous. Aucun nouveau tracé ou écran n'est attendu ; les résultats se lisent dans les réponses MCP.
3. Premier appel à `calculate_correlated_uncertainty_budget` :

   ```json
   {
     "output_unit": "millimeter",
     "coverage_factor": 2,
     "correlation_justification": "Modèle synthétique explicitement fourni pour vérifier le logiciel, sans qualification physique.",
     "components": [
       {"name": "localisation", "input_unit": "pixel", "standard_uncertainty": 0.3, "sensitivity_coefficient": 0.1, "source": "Donnée synthétique : 0,1 mm par pixel"},
       {"name": "reference", "input_unit": "millimeter", "standard_uncertainty": 0.04, "sensitivity_coefficient": 1, "source": "Donnée synthétique de référence"}
     ],
     "correlation_matrix": [[1, 0.5], [0.5, 1]]
   }
   ```

4. Attendre les contributions signées `0,03` et `0,04 mm`, `u_c=0,0608276253029822 mm` et `U=0,1216552506059644 mm`. La matrice, les sources et les unités doivent être restituées. Les deux composantes ne sont pas présentées comme indépendantes.
5. Changer les deux valeurs hors diagonale ensemble et comparer :

   | Corrélation rho | u_c | U à k=2 |
   |---|---:|---:|
   | 0 | 0,05 | 0,10 |
   | +0,5 | 0,0608276253029822 | 0,1216552506059644 |
   | −0,5 | 0,0360555127546399 | 0,0721110255092798 |
   | +1 | 0,07 | 0,14 |
   | −1 | 0,01 | 0,02 |

6. À rho `+0,5`, changer le deuxième coefficient en `−1` : `signed_contributions=[0,03,-0,04]`, `u_c=0,0360555127546399`, `U=0,0721110255092798`. Les signes ne doivent pas être remplacés par des valeurs absolues dans la combinaison.
7. Reprendre le premier appel avec `output_unit=degree` et la deuxième `input_unit=degree`, puis avec les deux unités `pixel`. Il s'agit de **deux nouveaux modèles synthétiques** : la première sensibilité est alors `0,1 degré/pixel`, puis `0,1 pixel/pixel`. Attendre les mêmes nombres, dans l'unité déclarée ; ce n'est pas une conversion de millimètres en degrés.
8. Vérifier neuf refus, en repartant chaque fois de l'appel valide : justification vide ; matrice de taille `1×2` ; matrice asymétrique `[[1,0.5],[0.4,1]]` ; diagonale `0,9` ; corrélation `1,1` ; k=0 ; champ supplémentaire `qualified=true` ; matrice indéfinie à trois composantes ; annulation nulle.

   Pour la matrice indéfinie, ajouter une troisième composante de nom distinct, u=0,04 et c=0, puis utiliser `[[1,-0.9,-0.9],[-0.9,1,-0.9],[-0.9,-0.9,1]]`. Elle doit être refusée même si la variance du vecteur demandé est positive. Pour l'annulation, utiliser rho `−1`, u de la deuxième composante `0,03` et ses coefficients inchangés à `+1` : les deux contributions s'annulent et le calcul est refusé.
9. Comparer les deux contextes après chaque appel : capture, révision, calibration, mesures, historique et inspection doivent rester identiques. Fournir la réponse MCP et l'étape en cas d'écart. Le résultat seul ne permet pas de déclarer une pièce conforme.

## Domaine numérique explicite

De 1 à 64 composantes, valeurs finies, u positives ou nulles, sources et noms non vides et noms uniques. La matrice doit être complète, exactement symétrique, de diagonale exactement 1, avec entrées dans `[-1,1]`. Aucune matrice n'est complétée, moyennée ou corrigée automatiquement.

Les contributions sont normalisées par leur maximum. Les lignes intégralement identiques ou opposées sont réduites exactement ; les corrélations parfaites `+1` et `−1` de cette forme sont prises en charge. Une factorisation de Cholesky à pivot diagonal maximal exige chaque pivot strictement supérieur à `64 epsilon n`. Certaines matrices mathématiquement valides mais singulières autrement, ou trop mal conditionnées, restent refusées par prudence. La norme est calculée depuis le facteur, avec sommes compensées et `hypot`. Les annulations inférieures ou égales au seuil renvoyé sont refusées, sans forcer une variance négative à zéro.

Les seuils numériques sont restitués ; ils ne sont pas des incertitudes métrologiques. Le validateur indépendant des composantes est réutilisé : ses limites de RSS et d'incertitude élargie s'appliquent même si une corrélation aurait réduit le résultat. Les sous-normaux restent soumis à l'arrondi IEEE-754. DPM-15 reste en cours pour l'application aux mesurandes réels et la qualification de leurs modèles ; aucune source de corrélation n'est estimée automatiquement.

## Vérifications exécutées

Le 10 novembre 2026, sur la branche `codex/DPM-15-correlated-budget`, issue de `dev dd9faf5` après PR #50 :

- `cargo test --locked correlated_uncertainty::` : **9 réussis, zéro échec, 394 filtrés**.
- `cargo test --locked uncertainty::` : **18 réussis, zéro échec, 385 filtrés** : les neuf nouveaux tests et neuf tests historiques. Soit **18 tests Rust distincts**, pas 27. Contrôles analytiques, signes, permutation, matrice entière indéfinie, singularités, unités, sources, sérialisation, tailles et échelles numériques.
- `scripts/verify_correlated_budget_mcp.py` sur hôte propriétaire **18771** : **17 cas réussis**, huit calculs et neuf refus. Contexte de mesure et inspection comparés intégralement après chaque appel, inchangés. Hôte de test ignoré par défaut puis exécuté explicitement : **1 réussi**, arrêté après recette. Serveur utilisateur 8765 non utilisé.
- Revue indépendante en lecture seule : aucun défaut concret relevé dans la propagation, les permutations, la validation complète de matrice ou le routage sans mutation.
- Compilation debug réussie : `cargo build --locked --target-dir output/dpm15-correlated-app`. 40 avertissements de code inutilisé existants ; 30 lors de la compilation des tests. Journal `output/dpm15-correlated-build.log`.

Preuves locales : `output/dpm15-correlated-tests.log`, `output/dpm15-correlated-related-tests.log`, `output/dpm15-correlated-mcp-20261110-094216/recipe/summary.json` et `rpc_evidence_001.json`, journaux de l'hôte dans le dossier parent. Le script crée son dossier de sortie et sa petite image sentinelle ; il refuse le port 8765. Pour reproduire, démarrer un hôte de test propriétaire puis exécuter depuis le dépôt :

```powershell
python scripts/verify_correlated_budget_mcp.py output/dpm15-correlated-recipe-nouveau --test-port 18771
```

Le dossier doit être inexistant. Les sorties restent locales, sans données de pièce industrielle. Aucun test physique, calibration, mesure manuelle, qualification de méthode, décision de conformité ou suite complète n'est revendiqué.
