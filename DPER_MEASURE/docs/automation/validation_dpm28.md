# DPM-28 — Recette logicielle composée UI / MCP / projets / rapports

Base testée : `f308ac814b9c49239dfc6eafe9346a77b9d711e5` (DPM-16 fusionné). Exécution finale : 2 novembre 2026. Cette recette vérifie le domaine assisté déjà implémenté ; elle ne qualifie aucune pièce réelle.

## Parcours couvert

| Critère | Preuve exécutée |
| --- | --- |
| Même observation UI/MCP | Événements réels egui : vue explicite, deuxième association parmi deux propositions, revues natives distinctes, trois repères, recalage, aperçu groupé, enregistrement et attestation documentaire fictive. Huit points de contrôle comparent le panneau rendu, le snapshot courant et GetState sans mutation de lecture. Une application indépendante rejoue réellement aperçu et enregistrement via l'adaptateur MCP sur les mêmes pixels et prérequis déjà revus ; résultat géométrique, preuves, valeur et état sont identiques. |
| Référence indépendante | Disque analytique de rayon 22,5 px et calibration indépendante de 0,1 mm/px : diamètre attendu 4,5 mm. La proposition du dessin ne détermine ni la calibration ni le rayon mesuré. Les limites [4 ; 5] mm et U totale 0,1 mm sont fixées avant observation. |
| Décisions distinctes | Quatre exigences fixes exercent conforme, hors tolérance, indéterminé et non évalué dans le panneau natif. Retirer explicitement l'attestation suspend la conformité sans changer la géométrie native. |
| Sauvegarde / réouverture | Sauvegarde réelle du projet, image et PDF source conservés, puis ouverture dans une application neuve. Les preuves importées et revues révoquées empêchent toute validation courante ; trois frames egui et GetState présentent les mêmes motifs. Les données historiques restent consultables. |
| Mutations et cas adverses | Parcours réseau MCP réel : éditions, annulation/rétablissement, dépendances, changement des octets du plan, mauvaise capture/révision, projets invalides et sept cas adverses de vision. Ajout d'un contour valide accompagné de faux champs de qualification/U/revue/conformité : champs ignorés, état et ajustement inchangés, aperçu ordinaire toujours fonctionnel. |
| Rapports | Exports PDF/PPTX et manifestes avant/après réouverture ; CSV/XLSX historiques. L'état sauvegardé est explicitement historique et n'accorde pas de verdict courant. Les notes distinguent contour vérifié logiciellement, indéterminé et non évalué. |

## Corrections découvertes par la recette

- Un aller-retour JSON peut modifier d'une unité de dernier bit un point calculé depuis centre + rayon. La comparaison des seules valeurs recalculées d'une preuve **importée** accepte désormais un écart relatif maximal de `2 * f64::EPSILON`, sans plancher en pixels ou tolérance physique. Quatre régressions vérifient arrondis, valeurs non finies, vraies modifications et comparaison toujours exacte en session courante. Les blocages d'import et les identités/générations/calibrations restent inchangés ; cette correction ne rend jamais une preuve importée courante.
- Les notes PDF/PPTX affichent le statut sauvegardé avec sa réserve historique. La présentation sans attestation dit « conformité physique non établie » au lieu de forcer « indéterminée » lorsque la tolérance manque et que l'état est « non évalué ».
- La vérité du raster synthétique MCP tient compte de la convention de centres de pixels lors du redimensionnement Pillow : `(coordonnée haute résolution + 0,5) / facteur - 0,5`. Fixture version 3, centre analytique (199,625 ; 159,625) px, défini indépendamment du résultat mesuré.

## Vérifications finales réellement exécutées

- `scripts/test_inspection_workflow.ps1` : **212 tests réussis, zéro échec, un hôte ignoré par défaut**, puis hôte exécuté explicitement avec succès. Sept tests supplémentaires depuis la base (deux parcours composés, quatre tests d'arrondis, un test de notes d'export).
- Compilation debug réussie : 17 avertissements existants ; les tests signalent 13 avertissements, dont le repli de liens physiques vers copie dans le cache de compilation.
- MCP réel : 18 outils découverts, sept cas adverses réussis et nouvelle vérification de fausse autorité sur contour valide réussie. Diamètre observé **4,207240008 mm**, vérité synthétique **4,200000 mm**, nominal du dessin **4,000000 mm** ; ces trois valeurs sont conservées séparément.
- Recette native : contour courant vérifié, décision conforme avec déclaration documentaire fictive explicite. Après réouverture : géométrie à vérifier, conformité indéterminée, raisons d'import/revue/qualification/U/qualité présentes ; aucun faux motif de modification dû au seul arrondi JSON.
- Sept PDF finaux rendus par Poppler et inspectés visuellement : **32 pages**, **6 804 mots** contrôlés, zéro mot hors page et aucune coupure ou superposition visible. Avertissement de rendu existant « No display font for Symbol », sans glyphe visiblement manquant. Les exports PPTX et limites de blocs sont exercés par les tests ; aucun rendu PowerPoint de bureau n'est revendiqué.
- Serveur de test propriétaire sur 18766 arrêté ; profil utilisateur inchangé. Le serveur utilisateur 8765 n'est pas utilisé. Les variables d'environnement sont restaurées et aucun service préexistant n'est arrêté.
- Relectures déléguées et revue de coordination ; correction des artefacts initialement non conservés et du contrôle du panneau réellement rendu après réouverture. Relecture croisée finale du correctif numérique et des libellés sans point bloquant.

Les essais intermédiaires sont conservés : première comparaison trop stricte après JSON ; premier parcours MCP arrêté par une vérité de centre raster incorrecte ; deuxième recette réussie avant détection du faux motif de modification historique. Le résultat ci-dessus vient exclusivement de `dpm28-run-03`, après correction.

## Reproduire et consulter les résultats

Voir [usage.md](usage.md), section « Recette logicielle composée (DPM-28) ». Le runner exige un dossier de sortie neuf dans DPER_MEASURE, conserve les journaux, projets synthétiques, PDF/PPTX, manifestes et synthèses machine, et échoue si un artefact attendu manque.

Exécution locale conservée, relative à DPER_MEASURE :

`output/delivery/DPM-28-workflow-acceptance/snapshot/DPER_MEASURE/output/automation-validation/dpm28-run-03/`

Inspection PDF locale : `output/delivery/DPM-28-workflow-acceptance/pdf-qa-final/`. La [preuve machine publiée](evidence/dpm28.json) contient les synthèses et empreintes des sources sélectionnées. Les sorties locales volumineuses ne sont pas livrées dans Git.

## Limites explicites

La recette native fournit l'interprétation textuelle et les propositions SVG afin d'isoler l'orchestration UI ; elle ne teste pas leur extraction PDF/OCR ni la lecture sémantique de l'annotation de l'artwork. Elle impose une qualité exploitable synthétique avant observation pour isoler les barrières métrologiques ; la réouverture réévalue la qualité réelle et bloque le disque lisse. Le parcours réseau séparé utilise l'extraction PDF et les contrôles qualité réels.

Le rejeu MCP indépendant reçoit les prérequis déjà revus nativement ; il ne simule pas une revue humaine autorisée par MCP. La qualification et U totale de la matrice de décision sont des déclarations **fictives de test**, sans calcul ni certification de U. Aucun essai de caméra, pilote graphique, fenêtre de bureau, pièce réelle ou reconnaissance universelle n'est revendiqué. La qualification physique **DPM-26** demeure distincte ; les développements sémantiques/généralistes encore au backlog ne deviennent pas terminés par cette recette.

Livraison limitée à DPER_MEASURE, sans changement de la branche ou de l'index utilisateur et sans worktree Git ajouté.
