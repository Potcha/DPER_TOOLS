# Architecture cible

## Comprehension du produit

Video Assistant est un **compilateur video** : le scenario exprime l'intention,
le Video IR la rend executable, le Video Compiler la transforme en operations
portables, puis un adapter NLE l'applique. Le produit ne doit jamais demander a
un LLM d'ecrire du code Resolve. Le controleur cible est l'utilisateur, qui
corrige le storyboard, les choix de rush et le plan avant execution.

## Couches

```text
UI / API
   |
Application services + jobs + versioning + audit
   |
AI Director -> Video IR <- Style Engine
   |
Media Intelligence -> Media Matcher
   |
Video Compiler -> Edit Plan
   |
NLE capability boundary (EditAdapter)
   |              |
DaVinci          FFmpeg / autres futurs adapters
```

Les dependances pointent vers l'interieur. `core`, `ir`, `edit_plan` ne
connaissent ni FastAPI, ni SQLite, ni FFmpeg, ni DaVinci.

## Modules principaux

| Module | Responsabilite | Ne fait pas |
|---|---|---|
| `ir` | Types, migration et validation du Video IR | Parlant a un NLE |
| `ai` | `LLMProvider`, AI Director, modifications ciblees | Calculs de timeline |
| `styles` | Profils, resolution des styles et templates | Cliches de montage |
| `media` | Catalogue et references de fichiers lourds | Copie systematique des rushs |
| `media_intelligence` | ffprobe, scenes, silence, transcription, frames, embeddings | Choix narratifs |
| `matching` | Requete hybride, scores, alternatives | Operations Resolve |
| `compiler` | Video IR + style + matches -> Edit Plan | API NLE |
| `adapters` | Validation des capacites et traduction NLE | Decisions creatives |
| `projects` | Agregats, commandes, versions | Stockage lourd |
| `jobs` | Workers, progression, annulation | Logique metier |
| `api` | HTTP contractuel | Regles de montage |
| `ui` | Workflow humain, comparaisons, chat cible | JSON principal |

## Stack retenue

- **Python 3.13+** : typage strict, scripting Resolve natif, ffmpeg/ffprobe et
  ecosysteme ML. Un seul processus de coeur suffit au MVP.
- **Pydantic 2 + JSON Schema 2020-12** pour les contrats externes. Le noyau
  actuel reste utilisable avec la bibliotheque standard.
- **FastAPI + Uvicorn** pour l'API metier et le support OpenAPI.
- **SQLite** en mode WAL pour les projets, versions, jobs et index; PostgreSQL
  pourra remplacer le depot sans changer les services. Les medias restent hors
  base, par chemin canonique + taille + hash + inode/date.
- **Redis optionnel** et une file interne SQLite au MVP. Le protocole `JobRunner`
  permettra Celery/RQ sans contrat HTTP change.
- **React + TypeScript** pour l'UI. Les ecrans restent orientes cartes de scene,
        timeline, comparaison de candidats et validation.
- **FFmpeg/ffprobe** pour les mesures deterministes; Whisper local ou un service
  de transcription derriere son interface, jamais dans le compilateur.

## Frontieres d'interfaces

1. `LLMProvider.complete_json` retourne un objet JSON qui doit passer la
   validation stricte. Une erreur de schema n'est jamais devinee.
2. `MediaAnalyzer.analyze` retourne un rapport versionne et incremental.
3. `MediaMatcher.find` retourne selection, alternatives, scores et explications.
4. `EditCompiler.compile` est pur et deterministe.
5. `EditAdapter.dry_run` annonce les operations supportees avant `apply`.

Les contrats Python correspondants sont dans `src/video_assistant/contracts.py`.

## API HTTP proposee

```text
POST /v1/projects
GET  /v1/projects/{project_id}
POST /v1/projects/{project_id}/scenario
POST /v1/projects/{project_id}/storyboard:generate
GET  /v1/projects/{project_id}/storyboard/{version}
PUT  /v1/projects/{project_id}/storyboard/{version}
POST /v1/projects/{project_id}/storyboard/{version}:duplicate
POST /v1/projects/{project_id}/media
POST /v1/projects/{project_id}/jobs:analyze-media
POST /v1/projects/{project_id}/matches:rebuild
GET  /v1/projects/{project_id}/matches
POST /v1/projects/{project_id}/edit-plan:compile
GET  /v1/projects/{project_id}/edit-plan/{version}
POST /v1/projects/{project_id}/edit-plan/{version}:apply
POST /v1/projects/{project_id}/edit:dry-run
POST /v1/projects/{project_id}/changes:natural-language
GET  /v1/jobs/{job_id}
POST /v1/jobs/{job_id}:cancel
```

Les actions longues recoivent `202 Accepted` et une ressource `Job`. Les actions
NLE demandent un `edit_plan_version` explicite et `dry_run=true` par defaut.

## Stockage

Un depot metier versionne conserve les documents JSON: scenario, Video IR,
styles, matchs, Edit Plan et journal de decisions. Chaque version est immutable
avec `parent_id`, hash de contenu, auteur (humain, IA, systeme) et resume des
changements. Les bases analytiques (chunks, transcriptions, embeddings, frames)
utilisent des identifiants d'asset et peuvent etre reconstruites.

## Robustesse

- validation atomique avant execution;
- chemin de fichier re-resolu et fichier verifie immediatement avant Resolve;
- mode `dry_run` obligatoire pour un plan non valide;
- sauvegarde/chargement du projet Resolve courant autour de l'application;
- timeline portant un identifiant de version pour choisir creation ou mise a jour;
- journalisation par operation avec `before`, `after`, cause et capacite utilisee;
- annulation cooperative entre operations, jamais en plein appel Resolve;
- erreurs structurees avec code, chemin, cause probable et action proposee.

## Stack decision risks

Resolve impose une synchronisation UI/processus et expose des operations
variables selon version/licence. L'adapter doit donc se limiter a la capacite
sondee publiquement, detecter les ecarts en `dry_run`, et proposer une liste
d'actions manuelles quand une primitive n'existe pas.

