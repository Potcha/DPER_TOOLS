# Livraison et revue Jira

Le code et les preuves réutilisables restent dans `DPER_MEASURE/`. Les captures de pièces, projets `.dperproj`, rapports de mesure générés, PDF industriels et sorties temporaires restent des données de travail locales, sauf demande explicite de publication.

## Pour chaque tranche cohérente

1. Créer une branche dédiée depuis `dev` dans le même dépôt : `feature/DPER_MEASURE/DPM-<id>-<sujet-court>`.
2. Faire des commits limités à cette tranche, avec la clé Jira dans chaque sujet : `feat(DPM-<id>): description courte` ou le type Conventional Commit approprié.
3. Ouvrir une PR vers `dev`. Mettre le ticket principal dans le titre (`DPM-<id>: ...`) et les tickets associés dans la description. Relier chaque ticket avec les clés Jira et le lien de PR dans un commentaire si l'intégration Jira-GitHub ne crée pas elle-même le lien.
4. Décrire les contrôles réellement exécutés, leurs résultats, les limites restantes et les fichiers de preuve inclus. Ne pas présenter un compte rendu local ou une capture de sortie comme une preuve livrée si le fichier n'est pas dans la PR.
5. Après ouverture de la PR et ajout des preuves, passer à `Revue en cours` uniquement les tickets dont le changement est prêt pour la validation humaine. Garder en `En cours` les travaux partiels, les essais physiques non exécutés et les recettes incomplètes.
6. Passer un ticket à `Terminé` après validation humaine et fusion de la PR. Si la revue demande des corrections, garder ou remettre le ticket en cours et pousser les corrections sur la même branche.

Une PR peut réunir plusieurs tickets dépendants lorsque cela forme une tranche intégrée vérifiable. Le ticket principal reste dans la branche, le sujet de commit et le titre de PR ; chaque ticket associé reçoit une référence explicite à la PR. Ne pas inclure en bloc les fichiers locaux simplement parce qu'ils apparaissent dans le contrôle de source : sélectionner les sources, documents de référence et jeux de test synthétiques nécessaires à la tranche.

## Première tranche

La première tranche `feature/DPER_MEASURE/DPM-14-metrology-evaluator` livre l'évaluateur déterministe des preuves et ses tests unitaires. Elle ne prétend pas intégrer tout le parcours MCP/UI ni remplacer une qualification physique. Les tickets d'intégration restent ouverts jusqu'à leurs propres PR et preuves.
