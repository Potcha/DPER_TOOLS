# Integration DaVinci Resolve

## Allee utilisee

La cible est le **DaVinci Resolve Scripting API** expose par
`DaVinciResolveScript`, avec l'API Fusion pour les controles de templates. Elle
est preferable aux automatisations UI car les operations sont nommees, lisibles
et inscrivables dans un journal. Un MCP n'est qu'un transport d'execution du
meme API: il ne doit pas devenir un contrat metier.

Le `DavinciResolveAdapter` est la seule classe autorisee a appeler ces objets.
`EditDocument` reste purement portable. Chaque operation passe par:

1. validation du plan;
2. verification des fichiers et capacites;
3. sauvegarde du projet Resolve courant;
4. `dry_run` rendant avant/apres et tout fallback;
5. execution tracee, appele par appele;
6. verification en lecture des proprietes cles;
7. sauvegarde, puis restauration du contexte utilisateur.

## PoC Phase C

Le script reversible `tools/davinci_resolve_poc.py` couvre:

| Capacite | Appel retenu | Statut |
|---|---|---|
| Creation projet | `ProjectManager.CreateProject` | Non execute |
| Import fichier | `MediaPool.ImportMedia` | Non execute |
| Creation timeline | `MediaPool.CreateEmptyTimeline` | Non execute |
| Ajout piste video/audio | `Timeline.AddTrack` | Non execute |
| Insertion clip | `MediaPool.AppendToTimeline` | Non execute |
| Trim + position | `AppendClipInfo.startFrame/endFrame/recordFrame` | Non execute |
| Titre | `InsertTitleIntoTimeline` | Non execute |
| Texte titre | `FusionComp.AddTool('TextPlus').SetInput('StyledText')` | Non execute |
| Transition | `TimelineItem.AddTransition('Cross Dissolve')` | Non execute |
| Niveau audio | `SetProperties(AudioVolumeEnabled, AudioVolume)` | Non execute |
| Verification | `GetStart/End/Source*/Properties` | Non execute |
| Nettoyage | restaurer puis `DeleteProject` | Non execute |

### Resultat experimental du 2026-09-28

Resolve Studio **21.1** est actif. Les declarations ci-dessus ont ete verifiees
dans les stubs officiels 21.1. Le media synthetique H.264/AAC 25 ips a ete cree
et valide avec ffprobe. En revanche, les trois tentatives d'execution du PoC ont
ete **arretees avant execution** par la couche
d'approbation automatique:

```text
responses_feature_not_supported: text.format type 'json_schema' is not supported
```

Ce n'est ni une erreur Resolve ni un refus de securite. Aucun projet Resolve n'a
donc ete cree ou modifie. Tant que cette execution n'est pas realisable, la
capacite reelle reste **non confirmee** et aucun adapter ne doit declarer une
operation comme supportee. Relancer le script apres correction du revueur et
conserver sa sortie JSON dans `artifacts/poc/davinci-capability-report.json`.

## Limites connues des declarations 21.1

- aucun `SetStart`, `SetEnd`, `SetSourceStartFrame` ou `SetSourceEndFrame` sur
  `TimelineItem`: le trim editable apres insertion n'est pas expose;
- le trim prouve doit donc etre resolu dans `AppendClipInfo` a l'insertion;
- pour un re-trim tardif, l'adapter proposera une reconstruction explicite,
  jamais un deplacement silencieux;
- les proprietes de texte des titres Fusion dependent du template et doivent
  etre cartographies par template, avec lecture de controle;
- les presets de couleur et transitions Fusion doivent faire l'objet de tests
  nommes avant d'entrer dans la bibliotheque de primitives.

## Contract de capacite

`EditAdapter.capabilities()` retourne pour chaque `OperationKind`:

- `supported`: operation explicable de bout en bout;
- `implementation`: primitive exacte utilisee;
- `limitations`: conditions, version, licence, fallback.

`apply` refuse une operation absente du rapport de `dry_run`. Une erreur est
mappee vers `AdapterError` avec operation, parametres masques, appel Resolve et
etat partiellement applique.

