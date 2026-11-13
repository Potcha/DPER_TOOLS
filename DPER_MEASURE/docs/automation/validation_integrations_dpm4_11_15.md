# Raccordements DPM-4 / DPM-11 / DPM-15

## Périmètre livré

Les moteurs existants restent la source des calculs. Cette livraison raccorde leurs résultats à la session, aux mesures natives, à l'interface et aux preuves sauvegardées.

- DPM-4 : `select_drawing_datum_support` enregistre une proposition repère/support avec ses sources. Le panneau **Repères et supports du plan** permet sa confirmation documentaire explicite et son retrait. MCP ne peut pas fabriquer cette confirmation. Une modification des sources suspend le lien ; une réouverture retire sa revue courante.
- DPM-11 : `inspect_native_diagnostic` prépare les extrémités de segment/d'arc ou une intersection depuis les pixels. L'enregistrement exige un aperçu identique et courant, rejoue le calcul et conserve les constructions sources. Une intersection est une construction ; un aperçu ne crée aucun tracé. Le panneau **Aperçus des extrémités et intersections** permet de recalculer puis enregistrer les propositions préparées via MCP.
- DPM-15 : `apply_observation_uncertainty` rattache le calcul existant de distance, angle, diamètre ou longueur d'arc à une observation native courante. La géométrie et l'étalonnage viennent de la session. Les incertitudes, sources et corrélations sont déclarées explicitement. Le panneau **Budget d'incertitude de l'observation** partage la même opération.

Le projet passe au schéma 22 et accepte le schéma 21. Le rapport conserve un résumé et le manifeste les preuves détaillées. Les informations importées restent historiques à réexaminer.

## Résultats réellement exécutés

Le 13 novembre 2026, 48 tests Rust distincts ont réussi, sans échec, sur les seuls filtres concernés :

| Filtre `cargo test --locked` | Réussis | Filtrés |
|---|---:|---:|
| `datum_binding` | 5 | 483 |
| `native_diagnostic` | 8 | 480 |
| `observation_uncertainty` | 8 | 480 |
| `session_schema_versions_are_supported` | 1 | 487 |
| `evidence` | 27 | 461 |

Ces filtres se recouvrent : 49 exécutions correspondent à 48 noms distincts. Journaux locaux : `output/integrations-{datum,native,budget,schema,evidence}.log`. Compilation des tests : 30 avertissements de code inutilisé.

`cargo build --locked` a réussi (38 avertissements), journal `output/integrations-build-final.log`. Une première compilation dans une sortie neuve avait échoué dans mozjpeg/lib.exe avec l'erreur Windows 1114 ; la compilation utilisant le cache existant a ensuite réussi. Aucune suite complète ni nouvel installateur n'a été exécuté.

La recette `scripts/test_integrations.ps1` a réussi ses 12 contrôles sur un hôte MCP dédié au port 18776 : aperçu sans tracé, refus d'enregistrement direct, observation et dépendances, budget analytique, refus des entrées périmées/injectées sans mutation, sauvegarde/export/réouverture, extraction PDF réelle par Poppler, proposition documentaire et refus d'une fausse revue MCP. L'hôte a été arrêté normalement et l'environnement restauré ; le serveur utilisateur 8765 n'a pas été utilisé.

Résultats synthétiques : segment de **140,000000 px / 1,400000 mm** ; budget de distance **U = 0,028565713714171416 mm**, à k = 2, pour u = 0,2 px par coordonnée et u relative d'échelle = 0,01, matrice identité. L'oracle analytique est indépendant du moteur de propagation.

Preuves locales : `output/integrations-mcp-20261113-085616-053/recipe/summary.json` et journaux RPC voisins. Le PDF `integrated.pdf` a été rendu et ses trois pages inspectées : résumé des preuves, vue de groupe, détail/tableau. Aucun chevauchement ou contenu coupé visible. Poppler signale la police Symbol sans glyphe visiblement manquant. Les données et rapports synthétiques restent locaux, ignorés par Git.

## Protocole de validation disponible dès la livraison

### Rejouer automatiquement

Depuis PowerShell :

```powershell
Set-Location 'O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE'
.\scripts\test_integrations.ps1
```

Le script crée un dossier horodaté sous `output`, utilise son hôte de test puis l'arrête. Attendre `passed: true` dans `recipe/summary.json`. Aucun test physique ni examen de la fenêtre Windows n'est déduit de ce résultat.

### Vérifier la fenêtre Windows (complément facultatif)

1. Sauvegarder le projet utilisateur avant de fermer l'ancienne instance. Lancer `O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE\target\debug\dper_measure.exe` ; l'installateur antérieur ne contient pas ces raccordements.
2. Ouvrir `O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE\output\integrations-mcp-20261113-085616-053\recipe\integrated.dperproj`. Vérifier le segment et les trois constructions sources, la valeur 1,400 mm et les preuves du groupe. Le budget rouvert est historique/suspendu : aucune conformité nouvelle ne doit apparaître.
3. Pour un parcours courant, demander au client MCP de préparer un aperçu avec `inspect_native_diagnostic`, `commit: false`, en reprenant `capture_id` et la révision courante depuis `get_measurement_context`. Les paramètres reproductibles sont dans `scripts/verify_integrations_mcp.py`. Dans **Aperçus des extrémités et intersections**, consulter les profils puis cliquer **Enregistrer**. Attendre un seul tracé natif et ses dépendances, sans conformité physique accordée.
4. Dans **Budget d'incertitude de l'observation**, sélectionner cette observation et reprendre les entrées synthétiques de la recette. Cliquer **Calculer et attacher le budget** : attendre U = 0,028565713714171416 mm. Modifier une construction source : l'observation dérivée et son budget doivent être suspendus. Les valeurs inconnues du modèle JSON ne doivent pas être remplacées arbitrairement.
5. Ouvrir `O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE\output\integrations-mcp-20261113-085616-053\recipe\datum-review.dperproj`. Réanalyser la page PDF si le catalogue de session manque ; préparer à nouveau la proposition via la recette MCP si la vue n'est plus courante. Dans **Repères et supports du plan**, examiner la déclaration et le support avant **Je confirme ce lien documentaire**. Attendre uniquement la confirmation documentaire ; **Retirer le lien** la retire. Une réouverture ne doit pas conserver une revue courante.
6. Exporter un PDF dans un nouveau fichier, vérifier le groupe, ses valeurs, les constructions et les raisons de suspension. La recette fournie a déjà vérifié automatiquement la sauvegarde et l'export ; la manipulation de la fenêtre Windows n'est pas revendiquée comme exécutée.

## Limites de clôture

Une confirmation documentaire ne qualifie pas un datum physique ou un contrôle GD&T. Un budget attaché reste un calcul déclaré : il n'alimente pas automatiquement l'incertitude native attestée ni une décision de conformité. Les sources d'incertitude et la méthode nécessitent la qualification physique DPM-26. Les résidus de contours ne sont pas utilisés comme substitut à cette qualification. Aucune conformité de pièce réelle, caméra ou GPU n'est revendiquée.

Cette PR rend les raccordements logiciels examinables ensemble ; elle ne clôt pas implicitement les critères physiques ou les familles hors domaine des tickets d'origine.
