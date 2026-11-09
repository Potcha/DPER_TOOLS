# DPM-15 — budget scalaire explicite d'incertitude

## Périmètre de cette tranche

L'outil MCP `calculate_uncertainty_budget` calcule un budget scalaire au premier ordre à partir de composantes fournies, supposées indépendantes : contribution `v_i = |c_i| u_i`, incertitude-type combinée `u_c = sqrt(sum(v_i²))`, puis incertitude élargie `U = k u_c`. L'accumulation est normalisée par la contribution maximale pour limiter les problèmes numériques. Ce calcul ne déduit ni composante, ni indépendance, ni facteur de couverture à partir du plan ou de la photo.

Chaque composante porte une incertitude-type `standard_uncertainty` dans `input_unit`, un coefficient signé `sensitivity_coefficient` dans **unité de sortie / unité d'entrée**, un nom et une source. Toutes les contributions résultantes ont l'unité de sortie. Le résultat conserve les unités du numérateur et du dénominateur, le signe du coefficient, la contribution et la source. Il n'effectue aucune conversion implicite : un coefficient pixel → mm doit être fourni explicitement et justifié hors de ce calcul.

Unités d'entrée : `millimeter`, `degree`, `pixel`, `dimensionless`. Sorties : `millimeter`, `degree`, `pixel`. Le radian et les autres unités ne sont pas pris en charge. Une justification d'indépendance est obligatoire ; **un texte fourni par le client ne prouve pas cette indépendance**. Les corrélations ne sont pas modélisées. Une demande contenant des champs de corrélation est refusée plutôt qu'ignorée.

De 1 à 64 composantes sont acceptées. Noms uniques après retrait des espaces, sources non vides, facteur k fini strictement positif, incertitudes-types finies positives ou nulles et coefficients finis sont requis. Un total nul et un dépassement numérique sont refusés. Un coefficient nul est conservé comme contribution inactive. Les composantes, coefficients et choix de k doivent provenir d'une méthode justifiée ; `k=2` n'est pas, à lui seul, une garantie de couverture à 95 %.

Le résultat porte `status: calculation_only`, `applied_to_measurement: false`, `source_qualification_verified: false`, `conformity_evaluated: false`. Il ne lit ni ne modifie la session active et ne rattache pas U à une mesure. Le budget historique d'étalonnage en millimètres reste inchangé. L'application à des mesurandes réels, les corrélations et la qualification restent à traiter dans DPM-15 et DPM-26.

## Protocole manuel fourni dès la livraison

1. Fermer l'ancienne instance pour libérer le port MCP 8765. Lancer `O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE\output\dpm15-app\debug\dper_measure.exe`, depuis la branche `codex/DPM-15-explicit-uncertainty` au commit indiqué dans la PR. Aucune image n'est requise. Si un projet est déjà ouvert, le conserver pour contrôler l'absence de modification.
2. Demander à ce chat : **« Exécute la recette DPM-15 des budgets d'incertitude via MCP »**. Le chat relève le contexte, effectue les calculs ci-dessous et compare les contextes après les appels. Pour un autre client MCP, appeler `calculate_uncertainty_budget` avec le JSON fourni.
3. Cas longueur : attendre les contributions **0,03 mm** et **0,04 mm**, `combined_standard_uncertainty = 0,05 mm`, puis `expanded_uncertainty = 0,10 mm` avec k=2. La première sensibilité doit rester **0,1 mm/px**, la seconde **−1 mm/mm**. La source et la justification doivent être restituées.
4. Cas angle : remplacer `output_unit` par `degree` et l'unité de la deuxième composante par `degree`. La sensibilité de la première devient explicitement **0,1 °/px** : ce sont d'autres données synthétiques, pas une conversion de millimètres en degrés. Attendre **0,05°** et **0,10°**. Refaire en `pixel`, en changeant également la deuxième unité, pour obtenir **0,05 px** et **0,10 px**. Pour chaque nombre, tolérance relative de recette `1e-12`.
5. Refus : tester successivement une incertitude-type −0,1 ; k=0 ; une justification vide ; `input_unit: radian` ; deux noms identiques après retrait des espaces ; un champ supplémentaire `correlations: [[1,0.5],[0.5,1]]` ; un champ `qualified: true` ; enfin une contribution calculée avec `standard_uncertainty: 1e308` et coefficient 10. Chaque appel doit renvoyer une erreur explicite. Selon le client, un refus du schéma est une erreur RPC plutôt qu'un résultat `isError`.
6. Vérifier que capture, révision, mesures, calibration et états Annuler/Rétablir restent identiques. Les réponses doivent conserver les quatre champs de non-application/non-qualification ci-dessus. Aucun nouvel écran de saisie ni tracé n'est attendu : cette tranche expose le calcul à l'IA via MCP.

### Données synthétiques du premier appel

```json
{
  "output_unit": "millimeter",
  "coverage_factor": 2,
  "independence_justification": "Données synthétiques indépendantes pour vérifier le calcul logiciel uniquement.",
  "components": [
    {
      "name": "localisation",
      "input_unit": "pixel",
      "standard_uncertainty": 0.3,
      "sensitivity_coefficient": 0.1,
      "source": "Sensibilité synthétique : 0,1 mm par pixel"
    },
    {
      "name": "reference",
      "input_unit": "millimeter",
      "standard_uncertainty": 0.04,
      "sensitivity_coefficient": -1,
      "source": "Référence indépendante synthétique"
    }
  ]
}
```

## Recette automatisée reproductible

`scripts/verify_uncertainty_budget_mcp.py` exécute trois calculs et huit refus sur un **hôte de test détenu par le lanceur**, port ≥18765. Il ouvre seulement une image sentinelle synthétique sur cet hôte pour pouvoir surveiller sa session. Ne pas lui donner le port 8765 de l'utilisateur. Exemple depuis le dépôt : `python .\scripts\verify_uncertainty_budget_mcp.py .\output\dpm15-manuel-auto --test-port 18771`. Le serveur de test doit avoir été démarré séparément ; le dossier de sortie doit être nouveau. Les réponses exactes et la synthèse sont conservées dans ce dossier. Le script ne démarre ni n'arrête une application.

## Résultats

- `cargo test --locked uncertainty::` : **9 réussis**, zéro échec, 376 filtrés ; sept nouveaux tests et les deux tests historiques. Sont couverts : formule analytique, signes, unités, zéros, doublons, bornes, champs inconnus, dépassements, grandes/petites échelles et valeurs sous-normales. Les tests compilent avec 30 avertissements de code inutilisé.
- Après correction numérique, transport MCP réel : **11 cas réussis** (trois calculs, huit refus), session inchangée après chaque appel. Preuve : `output/dpm15-mcp/final/recipe/summary.json` et `rpc_evidence_001.json`. Le processus dédié 18771 a terminé avec succès ; le serveur utilisateur 8765 n'a pas été utilisé.
- Compilation debug réussie dans `output/dpm15-app/`, avec 40 avertissements de code inutilisé. Relecture indépendante : un problème d'arrondi des valeurs sous-normales a été corrigé par normalisation et couvert par une régression ; correction relue sans autre blocage relevé.

Journaux locaux : `output/dpm15-uncertainty-tests.log`, `output/dpm15-mcp/` et `output/dpm15-build.log`. Seules les vérifications liées à cette évolution ont été exécutées ; la suite complète reste réservée au bilan final. Aucune qualification physique ni application d'U à une mesure réelle n'est revendiquée.
