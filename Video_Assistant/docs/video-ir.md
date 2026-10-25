# Video IR / Storyboard Executable

## Role

Le Video IR est l'actif central et le langage intermediaire de production. Il
decrit **quoi** le montage doit racontrer et montrer, sans imposer comment un NLE
le realise. Il est valide par `schemas/video-ir.schema.json` (JSON Schema
2020-12) et represente en Python par `VideoIRDocument`.

## Principes

- version explicite (`1.0.0`) et migrations obligatoires;
- identifiants stables (`project`, `scene`, `request`, `decision`);
- intentions, contraintes, alternatives et fallbacks;
- separation narration / visuel / graphique / audio / montage;
- provenance de chaque decision importante (`scenario`, `style`, `user`, `ai`);
- aucune propriete Resolve ni chemin de template Fusion obligatoire.

## Structure

```text
Document
  project.delivery.platform, width, height, fps, target duration
  scenes[]
    intent + requirement + purpose
    editing.target_seconds + pace + transitions
    narration.text + source + completeness rule
    visual.description + search + shots + duration + elements
    selections[] -> reusable media requests and fallbacks
    graphics[] -> semantic templates and timings
    audio -> voice priority, music profile/levels, natural sound
    constraints -> mandatory, forbidden, fallback
    decisions[] -> provenance, rationale, confidence
```

## Exemples reels

- `examples/video-ir/endives-au-jambon.json` : recette YouTube complete avec
  ingredient list, narration, B-roll, fallback et decisions auditees.
- `examples/video-ir/tarte-citron-shorts.json` : format vertical 45 secondes et
  rythme contraint.
- `examples/video-ir/gateau-chocolat-podcast.json` : discussion longue avec
  phrase obligatoirement preservee.

## Validation

La validation se fait en deux niveaux:

1. le JSON Schema verifie la forme externe pour tout langage/client;
2. `VideoIRDocument.from_dict` verifie les invariants utiles au compilateur:
   version, unicitite des scennes, plages de duree, confiance et enums.

```powershell
$env:PYTHONPATH = "src"
python -m video_assistant validate examples\video-ir\endives-au-jambon.json
```

## Evolution

Une nouvelle capacite ajoute un champ optionnel avec defini, puis une major de
schema si elle change l'execution. L'admission d'un document ne doit jamais
dependre de connaissances implicites du generateur.

