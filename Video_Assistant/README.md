# Video Assistant

Video Assistant est un compilateur de production video pilote par scenario. Il
transforme une intention narrative, un catalogue de medias et un style en un
**Video IR** (Storyboard Executable), puis compile ce document en **Edit Plan**
portable avant de l'appliquer dans un logiciel de montage.

```text
Scenario -> Video IR -> Media Match -> Edit Plan -> NLE Adapter -> Timeline
```

Le MVP cible les recettes YouTube 16:9 de la chaine Crossroads. Le coeur reste
independant de DaVinci Resolve : aucune classe metier ne reconnait un type
Resolve.

## Principe de conception

- l'IA decide **quoi** produire et remplit un Video IR valide;
- le code deterministe valide, calcule les timecodes, construit l'Edit Plan;
- les adapters traduisent uniquement les primitives portables vers un NLE;
- chaque choix peut porter une provenance et un score pour l'auditabilite;
- l'application dans un NLE passe par un plan sauvegarde et un mode dry run.

## Etat actuel

Le noyau local couvre deja la chaine deterministic avant Resolve :

- scenario de recette -> storyboard valide, avec generateur LLM abstrait et
  fallback de secours exploitable hors ligne;
- Video IR et Edit Plan versionnes, validables et portable;
- analyse reelle ffprobe/ffmpeg : metadonnees, silences et changements de plan;
- matching de rushs explique et alternative-aware;
- compilation pure vers insertions, trims, titres, overlays, sous-titres,
  transitions, couleur, musique et niveaux;
- stockage SQLite versionne et jobs annulables;
- exemples Crossroads et tests d'integration reelle.

Phase C : l'instrument de preuve Resolve est isole et reversible, mais les deux
executions ont ete arretees avant toute mutation par une erreur du controle
d'approbation automatique. L'adapter ne doit donc pas encore promettre les
primitives DaVinci comme experimentees.

## Commandes

```powershell
$env:PYTHONPATH = "src"
python -m video_assistant validate examples\video-ir\endives-au-jambon.json
python -m unittest discover -s tests -v
python tools\davinci_resolve_poc.py --help
```

Le schema JSON canonique se trouve dans `schemas/`. Les choix structurants sont
documentes dans `docs/architecture.md`, `docs/video-ir.md` et
`docs/davinci-integration.md`.

