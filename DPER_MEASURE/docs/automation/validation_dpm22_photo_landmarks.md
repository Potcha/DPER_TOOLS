# DPM-22 — Repères photo par clic (incrément partiel)

Base : PR #20 fusionnée, commit 460b0b54a9893d7f64fac5bd5ad8d1c6b791ae61.

Chaque ligne du brouillon dispose de « Choisir sur la photo ». Le clic renseigne uniquement ses coordonnées Image X/Y dans les pixels natifs, avec la transformation finale du visualiseur (zoom, déplacement et densité écran). Les coordonnées PDF restent manuelles. Aucun point n'est déduit du nominal, aucune mesure ni calibration n'est créée. L'application du recalage reste une action distincte utilisant Register.

Un clic hors image, sur la limite droite/basse, commencé hors du visualiseur, déplacé ou accompagné de navigation est refusé. La molette et le bouton du milieu restent utilisables. Échap, le bouton Annuler et cancel_active_action préservent la saisie précédente. La sélection possède tout le geste, jusqu'au relâchement après annulation ; elle empêche ainsi une mesure ou annotation parasite. Réarmer ou retirer une ligne ne réaffecte pas un ancien geste à un indice déplacé.

Les coordonnées ne sont acceptées qu'après nouvelle vérification de la capture, de la révision, du plan, des octets du PDF et des pixels image. Une source remplacée à taille et date identiques est également refusée. Les autres commandes natives/MCP sont bloquées jusqu'à la fin du geste, y compris les commandes caméra/projet/export et les fenêtres déjà ouvertes. Les fenêtres sont conservées puis réaffichées après la sélection.

## Vérifications exécutées

- cargo test --locked : **159 réussites, zéro échec, un hôte ignoré par défaut**. Huit nouveaux tests utilisent de vrais événements egui, le bouton de ligne et central_image : précision à densité 2, zoom 4 et déplacement ; limites/glissement/molette/pan ; annulation sur plusieurs frames par Échap et requête MCP ; contexte modifié ; réarmement/retrait ; disparition de l'image ; huit variantes MCP refusées tant que le geste est actif puis annulé mais maintenu.
- Le contrôle positif crée ensuite une mesure avec le même clic ordinaire : l'absence de mesure parasite ne provient pas d'un outil ou d'un contrôle de qualité inactif.
- cargo build --locked : compilation debug réussie, 18 avertissements existants (13 pour les tests). Aucun nouvel avertissement identifié.
- Parcours MCP complet avec transport réel et opérations applicatives dans l'hôte de test sans caméra ni fenêtre : **réussi**, 18 outils découverts, sauvegarde/rechargement, dépendances, historique, PDF/manifestes et sept cas adverses. Diamètre synthétique observé 4,207240 mm pour 4,200 mm attendus, nominal 4,000 mm. L'hôte ignoré est exécuté explicitement : une réussite, zéro échec.
- Port de test 18766 ; profil utilisateur inchangé, seul le processus créé pour l'essai est arrêté. Le serveur utilisateur 8765 n'est pas sollicité. Écoute Running et connexion directe vérifiées avant le scénario.
- Revue indépendante : trois défauts trouvés puis corrigés (relâchement après annulation, mutations MCP non gardées, fenêtres pré-ouvertes). Nouvelle revue sans constat bloquant.

Les empreintes des sources et journaux, les noms des tests et le résumé MCP sont dans [la preuve machine](evidence/dpm22_photo_landmarks.json). Les journaux bruts restent dans output/delivery/DPM-22-photo-landmarks. La validation porte sur le contenu source sélectionné depuis la base fusionnée ; elle n'utilise pas les changements mixtes de la branche utilisateur.

## Limites et suite

**DPM-22 reste En cours.** La sélection côté photo est disponible ; les coordonnées PDF sont encore saisies manuellement. La création native des vues/associations et le lancement des observations restent à intégrer. Les tests pointeur exercent egui et le visualiseur réel, pas la fenêtre Windows ni son pilote graphique. Les essais sont synthétiques : aucune qualification métrologique de pièce réelle ni conformité physique n'est revendiquée.
