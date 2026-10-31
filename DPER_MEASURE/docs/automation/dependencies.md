# Dépendances entre mesures

Les dépendances décrivent la provenance d’une mesure dérivée. Elles indiquent
quelles mesures existantes ont servi de sources ; elles ne décrivent ni une
équation géométrique ni une validation métrologique.

## Déclarer les parents

`source_measurement_ids` est facultatif. L’omettre, ou fournir une liste vide,
crée une mesure racine indépendante. Pour une mesure dérivée, fournir les
identifiants de tous ses parents :

- dans chaque élément de `measurements` pour `add_measurements` ;
- à la racine des arguments de `inspect_feature` ;
- à la racine des arguments de `inspect_bound_feature`.

Exemple d’ajout d’une mesure dérivée :

```json
{
  "capture_id": "capture-exemple",
  "expected_revision": 42,
  "measurements": [{
    "kind": "distance",
    "points": [{"x": 40, "y": 50}, {"x": 90, "y": 50}],
    "label": "Largeur issue du contrôle",
    "source_measurement_ids": [17]
  }]
}
```

Chaque parent doit exister dans la capture active et être frais. Les relations
issues de la vision demandent en outre une observation actuelle : une preuve
importée ou périmée ne peut pas servir de base à un nouvel enfant. Un PDF
source modifié invalide les descendants concernés lors de sa relecture par le
MCP ou du rafraîchissement du panneau d'inspection.
Les relations
sont une déclaration explicite de provenance ; l’application ne déduit pas les
parents à partir de la proximité, des coordonnées ou de la forme des mesures.
Créer un lien ne calcule rien automatiquement, ne modifie pas la géométrie de
l’enfant et ne valide aucune conformité.

## Fraîcheur et révisions

À la création, une mesure dérivée liée à des parents frais est elle-même fraîche.
Une modification de géométrie ou la suppression d’un parent invalide ses
descendants, de façon transitive. Les mesures sans lien avec ce parent restent
inchangées. Les générations de dépendance sont conservées hors de l’historique
géométrique : undo et redo restaurent des points, mais ne rétablissent jamais une
preuve devenue périmée.

Une mesure dérivée périmée expose `dependency_stale: true`, ne fournit plus de
composantes numériques actuelles et affiche « À recalculer · dépendances
modifiées ». Les comparaisons et exports omettent ses valeurs actuelles et ses
verdicts anciens. CSV/XLSX omettent sa ligne de mesure courante ; PDF/PPTX
peuvent conserver sa géométrie visible avec l’avertissement de recalcul.

## Suppression, restauration et projets

La suppression d’un parent est refusée tant que ses descendants subsistent.
Une suppression groupée peut inclure le parent et les descendants. Le système
supprime alors les enfants avant les parents ; undo les restaure dans l’ordre
inverse, afin que les parents existent avant leurs enfants. Cette restauration
ne rafraîchit pas les dérivés.

À l’ouverture d’un projet, les liens doivent former un graphe sans cycle, avec
des identifiants uniques, des parents présents et une capture commune. Tout
dérivé importé est marqué périmé, même si le JSON indique `dependency_stale:
false`. Les générations et déclarations de fraîcheur stockées ne constituent
pas une preuve faisant autorité. Les versions de schéma futures sont refusées.

Pour produire à nouveau un résultat courant, créer un nouveau nœud ou relancer
l’inspection avec ses parents frais et les preuves nécessaires. Aucun champ
client ne peut remettre `dependency_stale` à faux ni revalider une mesure.

L’ancien producteur de motifs présent dans l’arbre de travail local n’est pas
dans cette branche de développement. Les futurs producteurs doivent renseigner
leurs liens explicitement ; aucune dépendance ne peut être inférée des anciens
projets ou de leurs géométries.
