# DPM-22 — Éditeur natif de recalage explicite (incrément partiel)

Suite de la PR #19, basée sur son commit bd6f5adc24a2b9b9a5f5118d8b1b57c85aa5e6db. Aucun changement de l'algorithme de similitude ni de l'opération MCP Register.

Le panneau permet de préparer un brouillon de 3 à 64 couples plan/photo, saisir un seuil de résidu en pixels natifs et choisir éventuellement une vue déjà définie. Les champs acceptent une virgule décimale. Les coordonnées plan sont en points PDF ; les coordonnées image sont en pixels natifs, indépendants du zoom. Aucune coordonnée n'est générée à partir du nominal.

Le brouillon conserve capture, révision, identité du plan, contenu de l'image et empreinte réelle du PDF. L'application revérifie ces entrées avant de transmettre la capture et la révision préparées à l'opération partagée Register. Un changement de contexte refuse l'application sans actualisation silencieuse du brouillon. Les repères courants peuvent être repris explicitement après vérification de capture/image et de la vue éventuelle ; les résultats historiques incomplets sont refusés. Les résidus restent consultables dans le panneau existant. La calibration et les mesures ne sont pas créées ou modifiées par cet éditeur ; un recalage réussi invalide les anciennes observations selon les règles communes.

## Contrôles réellement exécutés

- Validation initiale : 148 tests Rust réussis, zéro échec ; compilation debug réussie, 18 avertissements existants. Résultats supplémentaires ci-dessous.
- Six nouveaux tests : égalité natif/opération partagée sur une similitude analytique indépendante (échelle 2, rotation 0, translation 10/20) et conservation de calibration ; refus des changements de révision/capture/plan/source/pixels ; refus atomique de données malformées, coordonnées hors image, nombre de repères ou seuil invalides ; image figée et absence d'action ; absence de reprise implicite sur une autre capture ; rendu egui sans services des états vide/éditable/périmé.
- Revue indépendante sans constat bloquant. Les branches de reprise avec catalogue de vue évincé et recalage historique sans seuil sont protégées mais ne sont pas explicitement exercées par ces six nouveaux tests.
- Lors de la validation initiale, aucun nouveau succès MCP de bout en bout ni essai graphique interactif du nouvel éditeur n'était revendiqué. Le parcours MCP et les interactions egui automatisés ont ensuite été exercés ci-dessous. Les trois essais utilisateur de la PR #19 portent sur le panneau PDF précédent, pas sur ce recalage.

Les preuves et empreintes sont dans [dpm22_registration.json](evidence/dpm22_registration.json). Les journaux restent locaux dans output/delivery/DPM-22-registration-ui. Un binaire debug et des données plan/photo synthétiques avec trois repères explicites sont préparés pour la recette utilisateur dans ce même dossier ; ils ne sont pas publiés.

## Travail restant

DPM-22 reste ouvert : saisie experte numérique, sans sélection des repères par clic. La définition des vues, les associations et le lancement des observations restent via MCP. Le recalage sans vue exige de vérifier soi-même page et repère du dessin. Une similitude ne remplace pas l'étalonnage physique, ne démontre aucune dimension hors plan et ne valide aucune pièce.

## Validation supplémentaire exécutée par Codex

151 tests réussis, zéro échec ; un hôte MCP de test est ignoré dans l'exécution ordinaire puis lancé explicitement avec succès. Trois nouveaux tests utilisent de vrais événements pointeur/clavier dans egui : Nouveau brouillon, saisie des douze coordonnées et du seuil, Application, puis vérification de la similitude analytique ; refus de saisie vide/seuil négatif ; bouton désactivé après réimportation. Ils n'appellent pas directement la préparation ou l'application pour réaliser les actions utilisateur.

Un défaut de collision de noms des fichiers temporaires en tests parallèles a été corrigé par un compteur atomique et l'identifiant du processus ; les identifiants de production ne changent pas. La première exécution (150 réussites/1 échec) reste dans les journaux.

Le parcours MCP complet réussit ensuite sur le port dédié 18766 avec le vrai transport et les opérations applicatives, hébergés dans un test sans caméra ni fenêtre Windows : 18 outils découverts, diamètre observé 4,207240 mm pour 4,200 mm attendus (nominal 4,000 mm), sauvegarde/rechargement, repères/résidus, dépendances, annulation/rétablissement, PDF/manifestes et sept cas adverses réussis. Profil utilisateur inchangé et processus de test arrêté. L'hôte exige un port dédié, un arrêt explicite, un serveur effectivement Running et une révision modifiée par le scénario ; son simple lancement ne constitue pas une réussite du parcours.

Le diagnostic a séparé un défaut de visibilité du port via CIM d'un défaut serveur : le statut de l'hôte confirme son écoute, puis une connexion directe permet le scénario. Un essai intermédiaire échoue sur le partage du fichier journal. Seul registration-headless-03 est compté comme parcours complet réussi.

Ces résultats automatisent le parcours logiciel sans déléguer cette recette à l'utilisateur. Ils ne démontrent pas les interactions avec la fenêtre du bureau, le pilote graphique, la caméra ou une pièce réelle. Les limites de DPM-22 (saisie numérique, création des vues et observations à intégrer) restent applicables.
