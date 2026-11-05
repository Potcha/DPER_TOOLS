# Protocole de qualification physique — DPM-26

## Objet et statut

Cette fiche prépare la qualification de la chaîne de mesure de DPER_MEASURE sur un domaine d'utilisation défini. Elle sépare la calibration de l'évaluation indépendante et distingue résolution d'affichage, erreur observée, répétabilité et incertitude.

**Statut : protocole préparé, essais physiques non réalisés.** Aucune exactitude, aptitude à l'emploi ou conformité d'une pièce n'est démontrée par ce document. DPM-26 reste ouvert jusqu'à l'acquisition, l'analyse des résultats et leur revue.

La qualification ne vaut que pour le matériel, les réglages, les conditions et le domaine consignés. Toute modification importante de caméra, objectif, grossissement, éclairage, traitement ou montage peut nécessiter une nouvelle qualification.

## Conditions préalables

Avant les prises de vue, renseigner et approuver les éléments suivants :

- **Référence indépendante** : artefact dimensionnel adapté au mesurande, avec valeur certifiée, unité, certificat ou source, date de validité et incertitude. L'artefact de vérification est distinct de la cote nominale de la pièce contrôlée.
- **Séparation calibration/évaluation** : ne pas utiliser le même élément de référence à la fois pour régler l'échelle et pour conclure sur l'erreur, sauf si le plan d'essai documente explicitement une méthode indépendante qui évite cette dépendance. Privilégier un artefact ou une dimension distincte pour la vérification.
- **Critères fixés avant mesure** : erreur maximale admissible, incertitude visée, règle de décision et usage prévu. Ne pas déduire ces limites des résultats observés.
- **Domaine revendiqué** : dimensions, positions dans le champ, orientations, grossissements, éclairages et autres conditions réellement couverts. Toute condition non testée reste hors domaine qualifié.
- **Traçabilité** : conserver les images originales, les projets DPER_MEASURE, les réglages, les valeurs saisies et les exports associés aux identifiants de chaque essai.

Si le certificat, l'unité, la validité, l'incertitude ou le critère d'acceptation manque, suspendre la conclusion au lieu de remplacer l'information par une hypothèse.

## Configuration à figer et à enregistrer

Ne modifier qu'un facteur à la fois lorsqu'on étudie son effet. Pour chaque série, consigner :

| Paramètre | Valeur / identification |
|---|---|
| Version et révision de DPER_MEASURE | À renseigner |
| Caméra et numéro d'identification | À renseigner |
| Objectif, grossissement et accessoires | À renseigner |
| Résolution, format et compression de l'image | À renseigner |
| Éclairage, intensité et géométrie | À renseigner |
| Distance de travail et montage | À renseigner |
| Mise au point et méthode de déclenchement | À renseigner |
| Traitement d'image activé et paramètres | À renseigner |
| Méthode et référence utilisées pour calibrer | À renseigner |
| Température / conditions ambiantes pertinentes | À renseigner |
| Opérateur, date et identifiant de série | À renseigner |

Ne pas corriger, accentuer ou redimensionner une image source sans enregistrer la transformation. Si le logiciel propose une correction d'affichage, conserver aussi le fichier source non modifié.

## Plan d'essai recommandé

La matrice ci-dessous est un plan de départ, pas un seuil de qualification imposé par le logiciel. Ajuster et approuver le nombre de points selon le domaine prévu et l'incertitude de la référence avant de commencer.

1. **Champ** : mesurer au centre et dans des zones réparties vers les bords/corners du champ utile. Noter les coordonnées ou une carte de position afin que les mêmes zones puissent être reprises.
2. **Orientation** : répéter avec l'artefact à plusieurs orientations représentatives (par exemple 0°, 90°, 180° et 270° si l'artefact et le montage le permettent). Noter l'angle réel ou la méthode de positionnement.
3. **Répétitions indépendantes** : pour chaque condition, retirer/repositionner l'artefact et reprendre une image. Une nouvelle lecture du même fichier ne compte pas comme une répétition de prise de vue.
4. **Étendue dimensionnelle** : inclure des dimensions couvrant l'étendue d'utilisation annoncée, lorsque l'artefact certifié le permet. Ne pas extrapoler une qualification au-delà des dimensions testées.
5. **Conditions optiques** : évaluer séparément les réglages qui peuvent varier en production (objectif, grossissement, éclairage ou distance), sans mélanger leurs effets dans une même série non identifiée.
6. **Contrôle indépendant** : appliquer la calibration définie, puis mesurer la référence indépendante avec la procédure prévue. Garder l'identité de l'élément de calibration et celle de l'élément de vérification séparées dans les relevés.

Une image inexploitable, un bord ambigu, un changement de réglage ou une perte de traçabilité doit être signalé comme essai invalide ou déviation; ne pas supprimer silencieusement le résultat.

## Fiche de référence et critères préalables

| Champ | À renseigner avant les essais |
|---|---|
| Identifiant de l'artefact de calibration | À renseigner |
| Identifiant distinct de l'artefact de vérification | À renseigner |
| Certificat / source et date | À renseigner |
| Caractéristique certifiée et valeur de référence | À renseigner |
| Unité et convention de mesure | À renseigner |
| Incertitude de référence, facteur k et couverture | À renseigner |
| Domaine dimensionnel visé | À renseigner |
| Positions et orientations revendiquées | À renseigner |
| Conditions optiques revendiquées | À renseigner |
| Erreur maximale admissible / exigence d'usage | À renseigner |
| Incertitude maximale visée, si définie | À renseigner |
| Règle de décision approuvée | À renseigner |
| Responsable de l'approbation des critères | À renseigner |

## Journal des mesures

Créer une ligne par image et par caractéristique mesurée. Conserver l'identifiant de l'image originale et celui du projet pour retrouver l'observation dans DPER_MEASURE.

| Essai | Image / projet | Position du champ | Orientation | Condition optique | Référence certifiée | Mesure DPER_MEASURE | Unité | Écart signé | Résolution affichée | Qualité / statut | Déviation ou commentaire |
|---|---|---|---|---|---:|---:|---|---:|---:|---|---|
|  |  |  |  |  |  |  |  |  |  |  |  |
|  |  |  |  |  |  |  |  |  |  |  |  |
|  |  |  |  |  |  |  |  |  |  |  |  |

Calculer l'écart signé comme **mesure − valeur de référence** dans la même unité. Garder les valeurs brutes; si une correction est appliquée, conserver également la valeur avant correction, la formule, sa justification et sa version.

## Synthèse et budget d'incertitude

Établir les statistiques par condition ou sous-groupe homogène avant de calculer une synthèse globale. Au minimum, rapporter le biais moyen et la dispersion des répétitions; préciser les données incluses et exclues avec leur justification. Ne pas confondre variation entre reprises, variation entre positions et variation entre conditions optiques.

| Source de contribution | Valeur / méthode d'évaluation | Incertitude-type | Distribution / diviseur | Sensibilité | Contribution à l'incertitude | Source / justificatif |
|---|---|---:|---|---:|---:|---|
| Certificat de la référence indépendante |  |  |  |  |  |  |
| Répétabilité des prises de vue et mesures |  |  |  |  |  |  |
| Résolution d'affichage / quantification |  |  |  |  |  |  |
| Détection du bord et ajustement géométrique |  |  |  |  |  |  |
| Calibration de l'échelle |  |  |  |  |  |  |
| Positionnement, orientation et mise au point |  |  |  |  |  |  |
| Autre contribution pertinente |  |  |  |  |  |  |

> Ne pas remplir un budget par des valeurs supposées. Documenter les corrélations si les contributions ne sont pas indépendantes. Ne déclarer une incertitude combinée ou élargie que si le modèle, les contributions, le facteur de couverture et la méthode sont justifiés.

| Résultat de synthèse | Valeur / commentaire |
|---|---|
| Biais par condition | À calculer après acquisition |
| Répétabilité par condition | À calculer après acquisition |
| Variation observée dans le champ | À calculer après acquisition |
| Variation selon l'orientation | À calculer après acquisition |
| Incertitude combinée / élargie (si justifiée) | À calculer après acquisition |
| Résolution d'affichage | À rapporter séparément |
| Domaine réellement couvert | À renseigner |
| Conditions et limites non testées | À renseigner |

## Règle de conclusion

- Comparer les résultats aux critères documentés **avant** la campagne, en appliquant la règle de décision approuvée.
- Indiquer séparément : qualification technique, domaine couvert, incertitude évaluée, aptitude à l'usage déclaré et, si applicable, conformité de la pièce. Une conclusion sur le système de mesure ne valide pas à elle seule une pièce.
- Une incertitude inconnue, une référence insuffisamment adaptée, des résultats contradictoires ou un domaine insuffisamment couvert donnent une conclusion **indéterminée / qualification suspendue**, pas une acceptation implicite.
- Ne jamais présenter le nombre de décimales ou la résolution de l'affichage comme une preuve d'exactitude.

## Revue et pièces à conserver

- Images originales et métadonnées, sans écrasement des sources.
- Certificats et identification des références de calibration et de vérification.
- Projets DPER_MEASURE, réglages, journaux et exports de chaque série.
- Tableau de calcul et budget d'incertitude avec versions des formules/fichiers.
- Déviations, essais invalides, reprises et raisons d'exclusion.
- Revue indépendante, nom/date, décision et domaine autorisé.

## État de réalisation

Ce document fournit le protocole et les gabarits de relevé. Les références, critères préalables, acquisitions physiques, calculs, revue indépendante et conclusion restent à fournir. Il ne constitue pas une preuve d'essai ni une fermeture de DPM-26.
