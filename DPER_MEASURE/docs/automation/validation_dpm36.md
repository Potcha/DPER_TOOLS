# DPM-36 — Déclarations d'unités et provenance

Date : 3 novembre 2026. Base : `6c12569a3feb17b6a7685584c0cbf7852d0a417a`, PR #27 fusionnée. Sous-tâche de DPM-3, qui reste ouvert.

## Résultat

Le lecteur considère toutes les unités reconnues d'une déclaration, et conserve les déclarations exactes avec leur page, boîte PDF et mots sources disponibles. « All dimensions in mm » fournit désormais des millimètres ; « Units: mm / inches » ne fournit aucune unité héritée. Des déclarations différentes sur plusieurs pages restent contradictoires, sans portée locale inventée. L'état et les sources sont consultables dans l'interface et le JSON MCP.

Les unités inline restent prioritaires. Les tolérances explicitement mixtes ne produisent aucune limite calculée ; la même garde s'applique à MIN/MAX, y compris lorsque seule la première borne porte une unité. Un marqueur OCR `[Page N]` conserve sa page. La proposition d'une unité sous conflit garde la trace originale et demande une confirmation documentaire distincte. Une profondeur reste non mesurable en 2D.

Les conversions héritées non résolues sont bloquées et la comparaison explique `drawing_unit_context_unresolved`. Le contour réellement observé peut rester disponible indépendamment du nominal. Les empreintes incluent le contexte unitaire ; les nouveaux projets utilisent le schéma 16 pour éviter sa perte silencieuse par un ancien exécutable. Les projets antérieurs restent lisibles sans provenance fabriquée.

## Validation exécutée

- 247 tests réussis, zéro échec, un hôte ignoré par défaut puis exécuté explicitement avec succès. Douze tests supplémentaires depuis la base : huit du lecteur d'unités, deux de l'interface, un de métrologie et un de persistance. Compilation debug réussie, 17 avertissements existants ; 12 côté tests. Les premières compilations signalent aussi le repli du cache vers copie.
- Vérification ciblée du lecteur : 27 tests réussis. Une première exécution de huit tests d'unités avait échoué sur la page du marqueur OCR simple ; le lecteur de marqueurs a été corrigé avant les vérifications finales. Trois contrôles ciblés de contexte documentaire ont ensuite réussi.
- Relecture indépendante et revue de coordination : priorité inline MIN/MAX et préservation du statut profondeur corrigées. La confirmation documentaire persistée est intentionnelle et ne remplace pas les portes de conformité physique.
- Onze scénarios PDF synthétiques réellement extraits via MCP : anglais/français, pouces, déclarations répétées cohérentes, conflits sur une ligne et deux lignes, ordre inversé, conflit sur deux pages, absence, unité inline sous conflit, tolérance mixte. Le cas conflictuel propose une unité sans confirmation puis sauvegarde/recharge le projet : conflit et provenance restent présents. Aucune mesure native créée, calibration indépendante inchangée, sources PDF intactes.
- Les douze pages de ces onze PDF ont été rendues et inspectées sur deux planches : textes et géométries présents, sans coupure visible. Aucun nouveau contrôle visuel de rapport ou de fenêtre de bureau/GPU n'est revendiqué.
- Parcours MCP préexistant réussi : 18 outils, sept cas adverses, conservation des repères, dépendances/historique, rejet de fausse autorité et exports. Les 19 cas PDF d'attache DPM-35 passent également. La recette native composée conserve sa parité UI/MCP et ses décisions suspendues au rechargement.
- Première recette complète interrompue par l'assertion du smoke restée au schéma 15, mise à jour à 16. Deuxième recette interrompue avant le corpus d'unités car le contexte synthétique précédent n'avait pas été sauvegardé. Sauvegarde dédiée ajoutée avant ouverture ; troisième recette complète réussie. Tous les processus propriétaires ont été arrêtés et les profils utilisateur conservés, y compris sur les échecs.

## Preuves et reproduction

Le runner `scripts/test_inspection_workflow.ps1` inclut désormais `verify_drawing_units_mcp.py`. Il exige un dossier neuf et un port de test dédié (18766 ici), conserve les échanges JSON-RPC et arrête son seul processus propriétaire. Le serveur utilisateur 8765 n'est pas utilisé.

Résultats locaux sous `output/delivery/DPM-3-unit-conflicts/snapshot/DPER_MEASURE/output/automation-validation/dpm36-run-03/` : `tests.log`, `build.log`, `unit-checks/units_summary.json`, `attachment-checks/attachment_summary.json`, `mcp/smoke_summary.json`, preuves natives et `isolation_check.json`. Les deux planches d'entrée sont dans `output/delivery/DPM-3-unit-conflicts/pdf-qa/`. La [preuve machine publiée](evidence/dpm36.json) contient synthèses et empreintes, sans publier les sorties locales.

## Limites

Le lecteur couvre les préfixes et unités pris en charge (mm, cm, m, pouces et degrés explicites), pas la sémantique générale des cartouches, les exceptions locales ni les doubles cotations générales. Une confirmation documentaire conservée dans un projet ne constitue pas une revue physique dans la session courante ; les observations importées restent historiques et à refaire. Aucune qualification de pièce, caméra, chaîne optique ou incertitude totale n'est revendiquée. DPM-3 reste ouvert. Branche et index utilisateur conservés ; aucun worktree Git créé, aucun fichier d'un autre outil ni donnée industrielle inclus.
