# DPM-20 — Limiter l’invalidation à la caractéristique revue

Modifier la case de confirmation d’une caractéristique invalidait les mesures
de toutes les caractéristiques. Le changement de confirmation invalide désormais
les observations liées à la caractéristique et propage l’invalidation à leurs
descendants. Les mesures d’une autre caractéristique gardent leur décision si
elles ne dépendent pas des observations touchées. Confirmer de nouveau la
caractéristique ne rétablit pas la validité des anciennes observations.

## Rejeu manuel

Depuis `O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE`, ouvrir l’application et charger
une image figée et étalonnée avec un plan contenant deux caractéristiques
observables. Confirmer les deux caractéristiques, les observer et enregistrer
leurs mesures. Créer une construction dépendante de chaque mesure.

Dans **Plan de contrôle et inspection IA**, retirer la confirmation de la
première caractéristique. Vérifier que sa mesure et sa construction deviennent
périmées et que la seconde caractéristique conserve son avancement. Confirmer
à nouveau la première et vérifier que le suivi demande une nouvelle observation.
Les anciennes preuves restent accessibles. `get_inspection_state` doit montrer
une observation courante pour la deuxième caractéristique.

## Vérifications exécutées

`cargo test --locked inspection` a réussi avec **132 tests**, zéro échec et un
test d’hôte HTTP ignoré. La nouvelle régression utilise deux contours
synthétiques enregistrés, leurs bases de décision et un descendant par mesure.
Elle contrôle l’avancement via l’adaptateur MCP, le statut dans les données
préparées pour l’export, la conservation des preuves et l’invalidation après
reconfirmation. Le journal est disponible dans
`O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE\output\dpm20-local-feature-final-tests.log`.

Cette livraison vérifie la branche de revue de la caractéristique. La recette
manuelle en fenêtre Windows et le transport HTTP dédié restent à exercer pour
ce changement. DPM-20 reste en cours pour l’orchestration complète.
