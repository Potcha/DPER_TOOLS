# Decoupage MVP

## Phase A - Analyse et architecture

Livrables: decisions de stack, couches, risques, stockage, API et tests. Cette
phase est posee dans `docs/architecture.md` et ADR.

## Phase B - Video IR

Livrables: schemas JSON 2020-12, modeles Python stricts, validation CLI et trois
storyboards reels. Cette phase est implementee et testee. Le compilateur,
l'analyseur FFmpeg, le matcher explique et le stockage versionne ont egalement
ete poussees jusqu'a des tests reels.

## Phase C - PoC DaVinci

Livrables: script reversible, media synthetique, carte de capacites et strategie
de fallback. Le script est pret; l'execution experimentale reste a faire apres
reparation du controle d'approbation (voir `davinci-integration.md`).

## Phase D1 - Services et projet

- repository de documents versionnes SQLite;
- agregat projet et commandes d'import;
- FastAPI, OpenAPI et jobs SQLite;
- audit journal et erreurs metier;
- UI premiere: nouveau projet, scenario, storyboard, versions.

## Phase D2 - Media Intelligence

- `ffprobe` deterministe et verification de disponibilite;
- extraction frames, detection de changements de plan, silences;
- transcription derriere `TranscriptionProvider`;
- index FTS lexical d'abord, embeddings ensuite;
- jobs annulables avec progression et diagnostic fichier.

## Phase D3 - Matching et compilation

- requetes `find_media` avec scores expliques;
- selection humaine et alternatives;
- Style Engine et profil Crossroads;
- compilation deterministe vers Edit Plan;
- comparaison et validation avant application.

## Phase D4 - Adapter DaVinci du MVP

- connexion et detection de version/capacite;
- creation/mise a jour de timeline;
- import, insertion et trim a l'insertion;
- cuts, trims simples, musique, ducking, volumes;
- titres/listes/sous-titres par templates valides;
- transitions simples, audit et dry run obligatoire.

## Critere d'acceptation

Sur une recette Crossroads et un corpus de rushs reel: import scenario, Video IR
modifiable, analyse, matching, Edit Plan lisible puis premiere timeline Resolve
exploitable sans operation manuelle destructive. La qualite n'est pas encore
celle d'un monteur final, mais chaque choix est explique et remplaceable.

## Hors MVP

Effets Fusion complexes, IA generative video, analyse emotionnelle, multicam,
QA final automatique et adapters Premiere/FCP/FFmpeg. Leur future presence ne
doit pas elargir prematurement le Video IR requis au MVP.

