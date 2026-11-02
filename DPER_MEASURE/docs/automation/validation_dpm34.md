# DPM-34 — Hypothèses d'attache texte / cercle dans une vue explicite

Base : `efdced26ee455a4e2794e7f9765ee0987917cadf` (DPM-28 fusionné). Sous-tâche de DPM-3 ; recette exécutée le 2 novembre 2026. Cet incrément classe des **hypothèses géométriques**, sans reconnaissance générale des fonctions du dessin.

## Comportement livré

Les candidats circulaires d'une vue explicite peuvent recevoir une trace d'attache entre une boîte de texte et un tracé ouvert rectiligne. Les deux extrémités doivent se trouver à au plus **2 points PDF** de la boîte et du contour ; cette marge ne dépend ni du nominal ni d'une calibration. La source doit être un seul sous-tracé simple et visible, continu, non rempli et sans sémantique de peinture inconnue. Une courbe aplatie n'est pas admise comme un trait rectiligne.

Chaque trace est conservée dans `BindingCandidate.evidence` sous le préfixe `endpoint_attachment_hypothesis:` : support, source, empreintes, extrémités, écarts, marge et correspondances concurrentes. L'interface native et le MCP présentent ces mêmes preuves. Une seule correspondance attribuable dans la vue peut donner priorité au candidat ; plusieurs supports, textes ou cercles touchés conservent les alternatives sans priorité d'attache. Aucun candidat n'est choisi ni revu automatiquement.

Les sources du périmètre et du support sont empreintées ; modifier le support ou le contexte impose un nouveau choix et une revue. Les projets ne reçoivent aucun pouvoir de revue supplémentaire et aucun schéma de stockage n'est ajouté. Les structures de requêtes MCP restent inchangées.

Budgets de l'étape : 32 supports de 32 points maximum, 65 536 comparaisons, 256 traces et 16 384 entrées de relations. Un dépassement ou une extraction incomplète provoque une abstention de toute l'étape d'attache, sans priorité partielle. Les candidats circulaires restent proposés selon les règles précédentes.

La relecture a révélé que des classes CSS, filtres, marqueurs et compositions d'opacité portés par un parent SVG pouvaient disparaître de la description du support. L'extracteur conserve désormais un avertissement à la racine et aux éléments descendants ; l'étape d'attache s'abstient. Les propositions géométriques précédentes sont conservées, sans prétendre résoudre ces effets.

## Vérifications réellement exécutées

- Vérification ciblée initiale : 14 tests d'association réussis (dont 8 nouveaux), puis test natif egui réussi. Après la correction des styles hérités, la recette complète finale exécute **224 tests réussis, zéro échec, un hôte ignoré par défaut**, puis cet hôte explicitement avec succès. Douze nouveaux tests depuis la base : neuf sur les attaches, deux sur les avertissements de présentation héritée, un sur le panneau natif.
- Compilation debug finale réussie : 17 avertissements existants ; 12 côté tests. Le premier lancement ciblé n'avait pas compilé à cause d'un dossier temporaire inaccessible ; il a été relancé avec un dossier local dédié. Les journaux de cet échec de préparation et des exécutions réussies restent conservés.
- Tests de géométrie : cercle cible plus éloigné du texte qu'un distracteur, translations/rotation/inversion/scientifique, cercles concentriques et tangents, supports concurrents, boîtes de texte superposées, trait hors vue ou déconnecté, pointillés/remplissage/fermeture/courbe/chemin composé/dégénérescence/découpage, styles hérités, budgets et modifications des sources. Le nominal ne filtre jamais les rayons et les ambiguïtés ne sont pas supprimées.
- Test natif par vrais événements egui : la trace du premier candidat est effectivement rendue ; le choix initial reste vide, le bouton sans choix/justification ne crée rien ; après clic explicite et saisie, la sélection conserve sa trace et reste non revue. Aucune mesure ni calibration n'est créée. Cette fixture fournit texte/SVG ; elle ne prétend pas tester l'extraction PDF.
- **Six PDF synthétiques réellement extraits via le transport MCP** : attache unique (2 cercles, 1 trace), sens inversé (2, 1), trait pointillé (2, 0), trait déconnecté (2, 0), support exclu de la vue (2, 0), deux supports vers des cercles concentriques (3, 2 traces ambiguës, sans priorité). Dans les cas uniques, le cercle lié par le support passe devant un cercle plus proche du texte. Les lectures sont déterministes, les sources PDF inchangées et toute sélection reste `reviewed: false`.
- Six pages de ces sources PDF rendues et inspectées visuellement : traits/textes/cercles correspondent aux cas annoncés, sans coupure visible. Cette inspection concerne les entrées synthétiques de DPM-34 ; aucun nouveau contrôle visuel de rapports ou de fenêtre GPU n'est revendiqué.
- Le parcours MCP préexistant reste réussi : 18 outils, sept cas adverses, fausse autorité sur contour valide sans effet, projets/rechargement, dépendances, historique et exports. Diamètre observé 4,207240008 mm pour une vérité synthétique 4,2 mm et un nominal 4,0 mm distincts.
- Serveur propriétaire de test sur 18766 arrêté et profil utilisateur inchangé ; serveur utilisateur 8765 non utilisé. Relectures croisées : correctif de style hérité et alignement des gardes de dossier entre runner et vérificateur ; aucun point bloquant restant relevé.

## Reproduire et inspecter

Le runner existant `scripts/test_inspection_workflow.ps1` inclut désormais `scripts/verify_drawing_attachments_mcp.py` avant d'arrêter son serveur de test. Voir [usage.md](usage.md) et [drawing_geometry.md](drawing_geometry.md). Un nouveau dossier de résultats est obligatoire ; aucune source existante n'est écrasée. Les sorties restent locales et les prérequis manquants font échouer la recette.

Exécution conservée, relative à DPER_MEASURE : `output/delivery/DPM-3-leader-hypotheses/snapshot/DPER_MEASURE/output/automation-validation/dpm34-run-01/`. Les résultats spécifiques sont dans `attachment-checks/attachment_summary.json`, avec échanges JSON-RPC et les six PDF ; l'isolation est décrite dans `isolation_check.json`. La [preuve machine publiée](evidence/dpm34.json) conserve synthèses et empreintes, sans publier les sorties locales volumineuses.

## Ce qui reste dans DPM-3

Cet indice géométrique ne reconnaît pas une flèche, un datum, une fonction de ligne au sens d'une norme, un réseau de segments séparés, une vue automatique ou les relations sémantiques générales entre cotes. Les unités et interprétations restent soumises aux règles et revues existantes. Une attache candidate n'est pas une observation photo ni une preuve de conformité ; DPM-26 reste distinct pour la qualification physique. DPM-3 demeure ouvert après cet incrément.

Livraison limitée à DPER_MEASURE, sans changement de branche/index utilisateur ni création de worktree Git. Les données réelles de pièces, plans industriels et rapports locaux ne sont pas incluses.
