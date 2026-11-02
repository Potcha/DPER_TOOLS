# DPM-22 — Parcours natif assisté de première phase

Base : PR #21 fusionnée, abe6e624b1a2baeb36be027b61d0ed104e2b3dca. Ce lot termine le raccordement UI-01 ; il ne prétend pas terminer l'automatisation généraliste ni qualifier physiquement des pièces.

## Comportement livré

Le panneau natif définit et remplace des vues sur les catalogues PDF analysés, avec choix explicite des caractéristiques et primitives. Il conserve toutes les alternatives d'association disponibles ; le choix d'un cercle et sa justification passent par DefineView/Candidates/Select, sans sélection ni revue automatique. Les brouillons conservent leur capture, révision, plan, source et catalogue ; toute modification impose une préparation/reprise explicite. Le contrôle réel des octets PDF détecte aussi un remplacement à taille/date identiques.

Après le recalage explicite déjà livré, l'opérateur prépare une observation par caractéristique et groupe. Les localisations libres droite/cercle/arc et les paramètres de recherche sont modifiables ; une association PDF impose sa géométrie et sa vue. Prévisualiser appelle Inspect/InspectBound sans créer de mesure native, mais conserve une trace d'aperçu dans l'historique. Enregistrer exige une prévisualisation des réglages courants, appelle les mêmes opérations avec commit=true et consomme le brouillon pour éviter un doublon. Les sources, pixels, calibration, qualité, révision et contexte complet sont revérifiés avant action.

Réévaluer utilise GetState et les règles communes. Les preuves périmées restent visibles avec leurs motifs ; leur affichage ne rafraîchit pas le brouillon et ne modifie pas les preuves sauvegardées. La revue manuelle vérifie aussi révision/action/source ; les corrections de type/unité utilisent désormais ProposeInterpretation partagé. Les résultats de recherches libres et liées sont suspendus de la même manière après modification du PDF dans l'UI et le MCP. La sélection d'une mesure pour correction, les dépendances, la sauvegarde et les rapports existants sont conservés.

## Critères d'acceptation

| Critère DPM-22 | Réalisation et preuve |
| --- | --- |
| Analyse, correspondances, zones et évaluation dans l'interface | Analyse PDF et repères existants ; nouveaux éditeurs de vues/associations et observation/réévaluation ; scénario pointeur complet de la vue jusqu'à la mesure dans un groupe. |
| Mêmes règles UI et MCP | Requêtes DefineView, Candidates, Select, Register, Inspect/InspectBound, ProposeInterpretation et GetState partagées ; assertion d'égalité de l'évaluation après modification des pixels/PDF. |
| Propositions modifiables avant observation | Sélection explicite des sources, justification et paramètres ; édition invalide l'aperçu et impose une nouvelle prévisualisation ; source périmée refusée. |
| Limites de première phase visibles | Extrémités non mesurées signalées ; droite limitée au support et longueur non enregistrable ; arc limité au rayon ; calibration indépendante et conformité physique suspendue. |

## Validation exécutée

- **171 tests Rust réussis, zéro échec, un hôte MCP ignoré par défaut**, ensuite exécuté explicitement avec succès. Douze nouveaux tests incluent les actions réelles de pointeur/clavier, le choix d'une alternative ambiguë, les refus de contexte/source/catalogue, l'édition d'aperçu, les groupes, le refus du doublon, les droites/arcs et l'égalité des suspensions UI/MCP.
- Un scénario natif unique enchaîne vue, association, revues séparées, saisie des repères, similitude, aperçu, enregistrement dans un groupe et réévaluation après modification. Le rayon observé diffère de 2,5 pixels de la proposition de dessin ; le nominal reste 20 mm, la calibration indépendante 0,1 mm/pixel reste inchangée. L'absence de tolérance donne « non évaluée », sans verdict physique.
- Compilation debug réussie, 18 avertissements existants (13 pour les tests). Aucun workflow CI n'est présent sous .github/workflows dans la base contrôlée.
- Parcours MCP avec transport réel et opérations applicatives : **réussi**, 18 outils, sauvegarde/rechargement, dépendances, historique, PDF/manifestes et sept cas adverses. Diamètre synthétique 4,207240 mm pour 4,200 mm attendus, nominal 4,000 mm. Port dédié 18766 ; profil utilisateur inchangé et processus de test arrêté. Instance utilisateur 8765 non sollicitée.
- Revue indépendante finale : aucun blocage sur les critères de première phase. Les premiers essais ont permis de corriger le montage de test (appel API qualité, ambiguïté de libellé, attentes de statut) ; leurs journaux sont conservés.

Les empreintes et détails figurent dans [la preuve machine](evidence/dpm22_complete_ui.json). Les journaux bruts restent dans output/delivery/DPM-22-complete-ui.

## Limites de la preuve

Les tests natifs installent des sources textuelles/vectorielles synthétiques et pilotent les panneaux egui ; l'extraction PDF et les exports sont exercés séparément via le transport MCP réel. Certains jeux de pixels passent explicitement la porte qualité pour isoler les interactions et l'ajustement ; cela ne qualifie pas les seuils d'acquisition. Aucun essai de fenêtre Windows, GPU, caméra ni pièce réelle n'est revendiqué. Les coordonnées PDF manuelles restent disponibles. L'appariement universel, les extrémités réelles, les jeux tangents à plusieurs butées et la qualification/incertitude physique restent dans les autres tickets.
