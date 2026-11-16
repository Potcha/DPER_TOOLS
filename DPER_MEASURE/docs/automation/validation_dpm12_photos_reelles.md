# DPM-12 — Essai sur trois captures réelles

Les essais utilisent les pixels originaux de trois captures de 1920 × 1080 px.
Les guides ont été définis à partir des zones visibles et, pour l’ouverture
du pied à coulisse, du relevé en pixels déjà conservé avec l’essai DPM-26.
Le plan nominal du plateau n’intervient pas. L’hôte Windows HTTP a exécuté
`inspect_visible_contour_spans` sur la version fusionnée `a5dcab1`.

| Capture et guide | Première série | Exploration après la première série |
| --- | --- | --- |
| Ouverture du pied à coulisse, deux bords droits et fond | À 3 px, refus des deux bords à cause de profils ambigus. Refus du fond par manque de transition. À 27 px, refus du guide centré entre les deux bords, avec 16 profils ambigus sur 16. | À 1 et 1,5 px, les deux bords sont observés et le fond est refusé. À 2 et 2,5 px, les trois guides sont refusés. |
| Circuit imprimé, arc du trou de fixation et guide intérieur incompatible | Les deux guides sont refusés à 5 px. | Les arcs restent refusés à 1, 2 et 3 px avec les guides essayés. Quatre guides droits sur le pourtour de la carte sont aussi refusés à 3 px. |
| Plateau usiné, courbe d’une ouverture en zone réfléchissante | Le guide quadratique est refusé à 6 px pour profils concurrents. | Même refus à 2 et 4 px. |

Les 14 appels couvrent 34 guides, dont quatre observations et 30 refus.
Chaque appel a conservé les états d’inspection et de mesure. Les empreintes
SHA-256 des trois images sources sont identiques avant et après chaque série.
Les deux instances de test ont été arrêtées normalement. La suite Rust du
même commit a donné 531 réussites et le test de l’hôte, ignoré par défaut,
a réussi lors de chaque série HTTP.

## Comparaison en pixels sur l’ouverture

Avec un rayon de 1 px, sur les abscisses 1100 à 1111, les 16 points de
chaque bord sont comparés au relevé historique par abscisse entière la plus
proche. L’écart signé est le point relevé par DPM-12 moins le point du relevé.

| Bord | Écart signé médian | Écart absolu maximal |
| --- | ---: | ---: |
| Supérieur | +0,245 px | 0,405 px |
| Inférieur | +0,075 px | 0,528 px |

Les guides et la comparaison utilisent le même relevé historique, issu des
mêmes pixels que le diagnostic. Ces écarts documentent la concordance locale
entre les deux traitements. Le rayon favorable a été trouvé pendant
l’exploration. Il reste à mesurer le comportement avec des guides fixés
avant l’essai sur de nouvelles acquisitions. La série ne fournit pas
d’observation positive sur un arc ou une courbe quadratique réels.

Une qualification physique demande des acquisitions répétées et une
référence dimensionnelle distincte de celle utilisée pour l’étalonnage.
Le [protocole DPM-26](protocole_qualification_physique_dpm26.md) décrit les
conditions et les preuves à réunir.

## Preuves locales

Le dossier `output/dpm12-real-images` conserve les scripts `recipe.py` et
`explore.py`, leur lanceur `run.ps1` et les journaux des deux séries.
Les fichiers `results.json` contiennent les guides, les réponses complètes
et l’identification des images. Les photos originales restent dans leur
dossier source.
