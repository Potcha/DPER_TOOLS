# AI Video App — Document de référence

## 1. Vision du produit

Créer une application IA capable de transformer un **scénario vidéo** en **Storyboard Exécutable**, puis de convertir automatiquement ce storyboard en **montage vidéo dans DaVinci Resolve**.

L'objectif n'est pas de créer un simple assistant qui suggère des coupes ou des transitions. L'application doit fonctionner comme un **moteur de production vidéo piloté par scénario**, dans lequel l'intention créative est structurée, versionnée puis exécutée de manière fiable.

### Principe général

```text
SCÉNARIO
  ↓
AI DIRECTOR
  ↓
STORYBOARD EXÉCUTABLE / VIDEO IR
  ↓
STYLE ENGINE
  ↓
MEDIA INTELLIGENCE
  ↓
MEDIA MATCHING
  ↓
EDIT PLAN
  ↓
VIDEO COMPILER
  ↓
DAVINCI RESOLVE ADAPTER
  ↓
TIMELINE DAVINCI RESOLVE
  ↓
QA / VALIDATION
  ↓
RENDU
```

L'humain reste le réalisateur.  
L'IA traduit les intentions créatives en données structurées.  
Le code déterministe exécute les opérations de montage.

---

## 2. Proposition de valeur

Le produit doit permettre à un utilisateur de fournir :

- un scénario ;
- des rushs vidéo ;
- des fichiers audio ;
- des images ;
- éventuellement une voix off ;
- éventuellement une musique ;
- un style vidéo prédéfini ou personnalisé.

L'application doit ensuite :

1. analyser le scénario ;
2. découper le contenu en séquences ;
3. générer un storyboard détaillé ;
4. convertir ce storyboard en format structuré ;
5. analyser et indexer les médias disponibles ;
6. rechercher les rushs correspondant à chaque séquence ;
7. générer un plan de montage ;
8. traduire ce plan en opérations exécutables ;
9. créer ou modifier une timeline DaVinci Resolve ;
10. appliquer le style choisi ;
11. vérifier le résultat ;
12. permettre des corrections ciblées sans régénérer tout le projet.

---

## 3. Principe architectural fondamental

Le système ne doit **jamais** transformer directement un scénario en commandes DaVinci Resolve.

Le cœur du produit est un format intermédiaire indépendant du logiciel de montage :

> **Video IR — Video Editing Intermediate Representation**

Analogie :

```text
Scénario = code source
Video IR = langage intermédiaire
Style = thème / règles visuelles et sonores
Video Compiler = compilateur
DaVinci Resolve = moteur d'exécution / rendu
```

Cette séparation permettra plus tard d'ajouter d'autres moteurs :

```text
DaVinci Resolve
Premiere Pro
Final Cut Pro
FFmpeg
CapCut
Autres NLE
```

L'actif stratégique principal du produit n'est donc pas l'intégration DaVinci elle-même, mais le **Video IR** et le moteur capable de le compiler.

---

## 4. AI Director

Le module **AI Director** transforme un scénario humain en Storyboard Exécutable.

### Responsabilités

- comprendre le scénario ;
- identifier la structure narrative ;
- découper en scènes ;
- définir la fonction de chaque scène ;
- estimer la durée ;
- déterminer les besoins visuels ;
- déterminer les besoins audio ;
- identifier les informations importantes ;
- déterminer les textes à afficher ;
- proposer les B-roll nécessaires ;
- préciser le rythme ;
- définir les contraintes ;
- proposer les transitions ;
- marquer les éléments obligatoires et optionnels ;
- produire le Video IR.

### Abstraction LLM

Le système ne doit dépendre d'aucun fournisseur unique.

Interface conceptuelle :

```text
LLMProvider
```

Adapters possibles :

```text
MiMoProvider
OpenAIProvider
AnthropicProvider
LocalModelProvider
OtherProvider
```

---

## 5. Storyboard Exécutable / Video IR

Le Video IR est le cœur du produit.

Il doit être :

- indépendant de DaVinci Resolve ;
- lisible par une machine ;
- compréhensible par un humain ;
- éditable ;
- versionné ;
- validable par schéma ;
- fortement typé ;
- extensible ;
- portable.

### Exemple conceptuel

```json
{
  "version": "1.0",
  "project": {
    "title": "Endives au jambon",
    "format": "youtube_16_9",
    "target_duration_sec": 480,
    "style": "crossroads_classic"
  },
  "scenes": [
    {
      "id": "ingredients",
      "title": "Présentation des ingrédients",
      "purpose": "Présenter rapidement les ingrédients nécessaires",
      "duration_target_sec": 14,
      "narration": {
        "source": "main_voice",
        "keep_complete_sentence": true
      },
      "visual": {
        "layout": "split",
        "primary": "preparation_broll",
        "search": [
          "endives",
          "jambon",
          "ingredients",
          "preparation"
        ],
        "preferred_shots": [
          "close_up",
          "medium"
        ],
        "shot_duration_sec": {
          "min": 2.5,
          "max": 4.0
        }
      },
      "overlay": {
        "type": "ingredient_list",
        "items": [
          "4 endives",
          "4 tranches de jambon",
          "50 g de beurre",
          "50 g de farine",
          "50 cl de lait"
        ]
      },
      "audio": {
        "voice_priority": true,
        "music_profile": "soft_cooking",
        "music_level_db": -20,
        "keep_natural_sound": true
      },
      "transition_out": {
        "type": "cross_dissolve",
        "duration_frames": 8
      }
    }
  ]
}
```

### Domaines à couvrir

Le schéma devra progressivement couvrir :

- projet ;
- scènes ;
- narration ;
- clips ;
- B-roll ;
- titres ;
- textes ;
- overlays ;
- sous-titres ;
- transitions ;
- effets ;
- couleur ;
- cadrage ;
- vitesse ;
- zoom ;
- musique ;
- SFX ;
- ambiance ;
- mixage ;
- contraintes ;
- priorités ;
- alternatives ;
- règles de fallback ;
- commentaires ;
- provenance de la décision ;
- niveau de confiance.

---

## 6. Style Engine

Le contenu et le style doivent rester séparés.

Le storyboard peut dire :

> Afficher la liste des ingrédients.

Le Style Engine détermine :

- police ;
- taille ;
- couleurs ;
- emplacement ;
- animation ;
- template Fusion ;
- transition ;
- durée ;
- comportement responsive ;
- conventions audio.

### Exemple de profil

```text
CROSSROADS_CLASSIC

COLOR
- rendu chaleureux
- contraste modéré
- tons chair naturels
- nourriture légèrement mise en valeur

EDITING
- plans de 3 à 6 secondes
- peu de jump cuts
- cuts simples en priorité
- rythme fluide

TRANSITIONS
- cut
- cross dissolve
- transitions discrètes

AUDIO
- voix prioritaire
- musique entre -18 et -22 dB
- sons naturels de cuisine conservés si intéressants

B-ROLL
- gros plans nourriture
- gestes de préparation
- ingrédients
- résultat final

TITLES
- templates Fusion Crossroads
```

### Fonctions souhaitées

Les styles doivent pouvoir être :

- créés ;
- clonés ;
- modifiés ;
- sauvegardés ;
- importés ;
- exportés ;
- partagés.

À terme :

> générer automatiquement un style à partir d'un ensemble de vidéos existantes.

---

## 7. Media Intelligence Engine

Le Media Intelligence Engine analyse et indexe les médias.

### Informations à extraire

Pour chaque vidéo/audio :

- nom ;
- chemin ;
- durée ;
- résolution ;
- codec ;
- framerate ;
- timecode ;
- transcription ;
- locuteurs ;
- silences ;
- scènes ;
- changements de plan ;
- frames représentatives ;
- objets ;
- personnes ;
- type de plan ;
- netteté ;
- exposition ;
- mouvement caméra ;
- qualité globale ;
- contenu parlé ;
- contenu visuel ;
- événements audio ;
- ambiance ;
- musique ;
- bruit ;
- tags sémantiques.

### Philosophie

Ne pas utiliser un LLM pour tout.

Privilégier les outils déterministes lorsque possible :

- FFmpeg ;
- métadonnées ;
- transcription ;
- détection de scènes ;
- extraction de frames ;
- embeddings ;
- analyse audio ;
- vision spécialisée.

Réserver les modèles multimodaux aux tâches de compréhension sémantique et de comparaison.

---

## 8. Media Matcher

Le Media Matcher relie les besoins du Video IR aux médias disponibles.

Exemple :

```text
Recherche :
Michel coupe les endives
Plan : close-up
Durée : 3 à 4 secondes
Qualité : meilleure possible
```

Résultat :

```text
Candidate 1
clip_023.mov
00:14:31.2 → 00:14:35.0
score : 0.94

Candidate 2
clip_028.mov
00:21:04.0 → 00:21:08.1
score : 0.86
```

Chaque candidat doit idéalement fournir :

- score global ;
- score sémantique ;
- score visuel ;
- score technique ;
- contraintes satisfaites ;
- raisons de sélection ;
- éventuels avertissements.

L'utilisateur doit pouvoir remplacer un choix automatique.

---

## 9. Edit Plan

Le **Edit Plan** représente le montage concret à effectuer.

Il est généré à partir de :

```text
Video IR
+
Style
+
Media Matches
```

Il doit être :

- lisible par machine ;
- inspectable ;
- versionné ;
- modifiable ;
- indépendant de DaVinci.

### Exemples d'opérations abstraites

```text
InsertClip
TrimClip
MoveClip
SetTrack
SetSpeed
AddTransition
AddTitle
AddOverlay
AddSubtitle
AddMusic
SetVolume
DuckMusic
NormalizeAudio
ApplyColorPreset
AddFade
RemoveSilence
```

---

## 10. Video Compiler

Le Video Compiler transforme le Video IR en Edit Plan.

Il ne doit pas connaître les détails de DaVinci Resolve.

### Responsabilités

- résoudre les durées ;
- organiser la timeline ;
- transformer les intentions en opérations ;
- appliquer les règles du style ;
- gérer les contraintes ;
- gérer les fallbacks ;
- produire un plan cohérent ;
- éviter les conflits temporels ;
- signaler les décisions impossibles ou ambiguës.

---

## 11. DaVinci Resolve Adapter

Le module :

```text
DavinciResolveAdapter
```

traduit les opérations génériques en commandes compatibles avec DaVinci Resolve.

### Technologies à étudier

- DaVinci Resolve Scripting API ;
- Python ;
- Lua ;
- Fusion ;
- API exposées par Resolve ;
- MCP lorsque pertinent ;
- scripts externes ;
- templates Fusion.

### Règle importante

Aucune logique métier ne doit être directement intégrée dans l'adapter DaVinci.

L'adapter est uniquement une couche d'exécution.

---

## 12. Bibliothèque de primitives

Le système doit utiliser en priorité des primitives testées et réutilisables.

Exemples :

```text
clip.insert()
clip.trim()
clip.move()
clip.speed()
clip.crop()

transition.cross_dissolve()
transition.fade()
transition.whip()
transition.zoom()

title.standard()
title.ingredients()
title.chapter()

subtitle.create()
subtitle.style()

audio.normalize()
audio.duck_music()
audio.fade()
audio.keep_ambient()

broll.insert()

color.apply_profile()

silence.remove()
```

L'objectif est de ne pas laisser le LLM recréer continuellement des scripts différents pour la même opération.

Principe :

> IA = décide quoi faire  
> Code déterministe = décide exactement comment l'exécuter

---

## 13. Templates

Prévoir une bibliothèque de templates :

- intro ;
- outro ;
- titre principal ;
- lower third ;
- liste d'ingrédients ;
- étapes ;
- chapitres ;
- CTA ;
- split screen ;
- picture-in-picture ;
- sous-titres ;
- transitions ;
- overlays.

Exemple :

```text
crossroads_title_v1
crossroads_ingredients_v2
crossroads_step_v1
crossroads_subtitle_v1
```

---

## 14. UX cible

### Étape 1 — Nouveau projet

- nom ;
- plateforme ;
- résolution ;
- ratio ;
- durée cible ;
- framerate.

### Étape 2 — Scénario

- écrire ;
- coller ;
- importer.

### Étape 3 — Storyboard

Bouton :

```text
Générer le storyboard
```

Afficher :

- scènes ;
- durée ;
- narration ;
- visuels ;
- B-roll ;
- textes ;
- transitions ;
- audio ;
- contraintes.

L'utilisateur peut modifier chaque élément.

### Étape 4 — Style

Choisir :

- style prédéfini ;
- style personnel.

### Étape 5 — Médias

Ajouter :

- vidéos ;
- audio ;
- images ;
- voix off ;
- musique.

Afficher la progression de l'analyse.

### Étape 6 — Media Matching

Afficher pour chaque scène :

- média choisi ;
- timecode ;
- alternatives ;
- score ;
- raisons.

### Étape 7 — Edit Plan

Présenter un résumé clair du montage prévu.

### Étape 8 — DaVinci Resolve

Actions :

```text
Créer la timeline
Mettre à jour la timeline
Dry Run
```

### Étape 9 — Validation

Afficher :

- médias manquants ;
- scènes incomplètes ;
- conflits ;
- avertissements ;
- erreurs ;
- propositions de correction.

---

## 15. Interaction en langage naturel

Un chat doit permettre des modifications ciblées :

```text
Raccourcis l'introduction.
Garde davantage de sons naturels.
Remplace le B-roll de la scène 4.
Utilise le deuxième candidat.
Diminue la musique de 3 dB.
Supprime les plans de plus de 6 secondes.
Utilise des transitions plus discrètes.
```

La demande doit modifier uniquement les éléments concernés.

Éviter de régénérer tout le projet lorsqu'une modification locale suffit.

---

## 16. Versioning

Versionner notamment :

```text
Scenario v1
Storyboard v1
Storyboard v2
Style v1
Media Analysis v1
Edit Plan v1
Edit Plan v2
```

Fonctions :

- historique ;
- comparaison ;
- duplication ;
- rollback ;
- audit.

---

## 17. Dry Run

Le mode **Dry Run** doit générer :

- Video IR ;
- Media Matching ;
- Edit Plan ;
- avertissements ;

sans modifier DaVinci Resolve.

Ce mode est essentiel pour :

- tester ;
- déboguer ;
- valider ;
- comparer des variantes ;
- sécuriser l'exécution.

---

## 18. Auditabilité

Le système doit pouvoir répondre à :

- pourquoi ce clip a-t-il été choisi ?
- pourquoi cette transition ?
- quelle règle de style a été appliquée ?
- quelle décision vient du scénario ?
- quelle décision vient du style ?
- quelle décision vient de l'IA ?
- quelle décision vient d'une règle déterministe ?
- quel était le niveau de confiance ?

Les décisions importantes doivent être journalisées.

---

## 19. Gestion des erreurs

Cas à gérer :

- média manquant ;
- chemin invalide ;
- clip déplacé ;
- durée insuffisante ;
- aucun B-roll correspondant ;
- fichier incompatible ;
- API IA indisponible ;
- modèle indisponible ;
- DaVinci fermé ;
- API Resolve indisponible ;
- erreur pendant l'application ;
- template manquant ;
- rendu interrompu ;
- erreur de timecode ;
- perte de connexion.

Ne jamais échouer silencieusement.

---

## 20. Jobs longs

Prévoir des jobs asynchrones pour :

- transcription ;
- génération de proxy ;
- extraction de frames ;
- analyse visuelle ;
- embeddings ;
- détection de scènes ;
- analyse audio ;
- compilation ;
- rendu ;
- QA final.

L'interface doit montrer :

- état ;
- progression ;
- logs ;
- erreurs ;
- possibilité de relancer.

---

# MVP — Crossroads

## 21. Cas d'usage initial

Le premier MVP cible les vidéos de cuisine YouTube de la chaîne Crossroads.

Structure fréquente :

```text
INTRO
PLAT TERMINÉ
PRÉSENTATION
INGRÉDIENTS
ÉTAPE 1
ÉTAPE 2
ÉTAPE 3
CUISSON
DRESSAGE
DÉGUSTATION
OUTRO
```

Ce format est adapté au MVP car il est fortement structuré et répétable.

---

## 22. Fonctions du MVP

Le MVP doit supporter :

- création d'un projet ;
- import / collage d'un scénario ;
- génération du Video IR ;
- édition du storyboard ;
- style Crossroads ;
- import des rushs ;
- transcription ;
- détection des silences ;
- B-roll simple ;
- Media Matching ;
- cuts ;
- trims ;
- déplacement de clips ;
- ajout de musique ;
- ducking ;
- niveaux audio ;
- titres ;
- liste d'ingrédients ;
- sous-titres ;
- transitions simples ;
- création de timeline DaVinci Resolve.

### Hors MVP

Ne pas prioriser :

- montage multicam avancé ;
- Fusion complexe ;
- génération vidéo IA ;
- analyse émotionnelle avancée ;
- dizaines de styles ;
- plusieurs NLE ;
- marketplace ;
- color grading autonome complexe.

---

## 23. Critère de réussite du MVP

Le workflow suivant doit fonctionner de bout en bout :

1. créer un projet ;
2. coller le scénario ;
3. choisir Crossroads ;
4. importer les rushs ;
5. lancer l'analyse ;
6. obtenir un Storyboard Exécutable ;
7. corriger le storyboard si nécessaire ;
8. lancer le Media Matching ;
9. obtenir un Edit Plan ;
10. effectuer un Dry Run ;
11. cliquer sur `Créer dans DaVinci Resolve` ;
12. obtenir une première timeline réellement exploitable.

Le montage n'a pas besoin d'être parfait.

Le succès du MVP est atteint s'il réduit significativement le temps de montage manuel.

---

# Architecture logicielle

## 24. Modules proposés

```text
apps/
  web/
  desktop/

packages/
  core/
  video-ir/
  ai/
  scenario/
  storyboard/
  styles/
  media/
  media-intelligence/
  media-matcher/
  edit-plan/
  video-compiler/
  templates/
  adapters/
    davinci/
  storage/
  jobs/
  logging/
  shared/
```

Cette structure est indicative.

Le découpage doit privilégier :

- modularité ;
- typage ;
- testabilité ;
- indépendance des couches ;
- interfaces stables.

---

## 25. Modèle de dépendances

```text
UI
 ↓
Application Services
 ↓
Domain / Video IR
 ↓
Compiler / Matching / Styles
 ↓
Adapters
 ↓
DaVinci / Filesystem / AI Providers / Database
```

Le domaine ne doit pas dépendre :

- de DaVinci ;
- d'un modèle IA spécifique ;
- d'une base de données particulière ;
- de l'interface utilisateur.

---

## 26. API interne indicative

```text
POST /projects
GET  /projects/{id}

POST /projects/{id}/scenario
POST /projects/{id}/generate-storyboard

GET  /projects/{id}/storyboard
PUT  /projects/{id}/storyboard

POST /projects/{id}/media
POST /projects/{id}/analyze-media

POST /projects/{id}/match-media

POST /projects/{id}/compile

POST /projects/{id}/dry-run

POST /projects/{id}/davinci/apply

POST /projects/{id}/render

GET /jobs/{id}
```

Ces endpoints sont indicatifs et devront évoluer avec l'architecture réelle.

---

# Principes de développement

## 27. Règles

1. Ne jamais coupler le Video IR à DaVinci.
2. Ne jamais laisser un LLM exécuter directement des actions critiques sans validation.
3. Utiliser des schémas stricts pour tous les échanges IA structurés.
4. Versionner les formats.
5. Valider les sorties LLM.
6. Préférer les primitives déterministes.
7. Journaliser les actions importantes.
8. Concevoir le Dry Run dès le départ.
9. Ajouter des tests pour chaque primitive DaVinci.
10. Prévoir les fallbacks.
11. Ne jamais supposer qu'une fonction DaVinci est disponible sans l'avoir testée.
12. Séparer la décision créative de l'exécution technique.
13. Permettre la correction locale.
14. Éviter les régénérations complètes inutiles.
15. Favoriser les modules remplaçables.

---

## 28. Risques techniques principaux

### API DaVinci incomplète

Certaines actions de l'interface peuvent ne pas être exposées.

Réponse :

- POC précoce ;
- abstraction par primitives ;
- Fusion lorsque nécessaire ;
- stratégies alternatives clairement isolées.

### Analyse vidéo coûteuse

Une analyse image par image est trop lourde.

Réponse :

- détection de scènes ;
- extraction sélective de frames ;
- proxies ;
- embeddings ;
- analyse multimodale ciblée.

### Hallucination LLM

Réponse :

- JSON Schema ;
- validation stricte ;
- références explicites aux médias existants ;
- aucune invention de timecodes ;
- vérification déterministe.

### Dépendance à un modèle

Réponse :

- `LLMProvider` ;
- providers interchangeables.

### Montage non déterministe

Réponse :

- primitives ;
- Edit Plan ;
- seed/versioning si applicable ;
- logs ;
- Dry Run.

---

# Roadmap

## 29. Phase 0 — Validation DaVinci

Créer un POC permettant :

- connexion à Resolve ;
- lecture d'un projet ;
- création d'une timeline ;
- ajout d'un clip ;
- trim ;
- positionnement ;
- piste vidéo ;
- piste audio ;
- titre ;
- transition ;
- niveau audio ;
- sauvegarde.

Objectif :

> déterminer précisément les primitives fiables.

---

## 30. Phase 1 — Video IR

Définir :

- schema v1 ;
- validation ;
- exemples ;
- sérialisation ;
- migrations ;
- tests.

---

## 31. Phase 2 — AI Director

Entrée :

```text
Scenario
```

Sortie :

```text
Video IR
```

Créer des tests avec plusieurs scénarios de recettes.

---

## 32. Phase 3 — Style Engine

Créer :

```text
crossroads_classic
```

ainsi que les templates nécessaires.

---

## 33. Phase 4 — Media Intelligence

Première version :

- metadata ;
- transcription ;
- silences ;
- scènes ;
- keyframes ;
- tags visuels simples.

---

## 34. Phase 5 — Media Matcher

Relier les besoins du Video IR aux clips.

---

## 35. Phase 6 — Video Compiler

Transformer :

```text
Video IR
+ Style
+ Matches
```

en :

```text
Edit Plan
```

---

## 36. Phase 7 — DaVinci Execution

Exécuter l'Edit Plan dans Resolve.

---

## 37. Phase 8 — UI complète

Créer le workflow utilisateur complet.

---

## 38. Phase 9 — QA

Générer un rendu proxy puis analyser :

- écran noir ;
- clip manquant ;
- texte coupé ;
- silence anormal ;
- audio trop fort/faible ;
- rupture de rythme ;
- erreur de transition ;
- média incorrect.

---

# Évolutions futures

## 39. Style Learning

Analyser les anciennes vidéos d'un utilisateur pour créer :

```text
user_style_v1
```

Exemples de paramètres appris :

- durée moyenne des plans ;
- fréquence de B-roll ;
- types de transitions ;
- style des titres ;
- mixage ;
- musique ;
- cadrage ;
- rythme.

---

## 40. Audio Intelligence avancée

Détecter et comprendre :

- crépitement ;
- eau ;
- couteau ;
- mixeur ;
- cuisson ;
- ambiance ;
- réactions ;
- bruit parasite ;
- musique ;
- silence utile.

---

## 41. Analyse visuelle avancée

Comparer plusieurs plans pour choisir :

- le plus net ;
- le mieux exposé ;
- le plus esthétique ;
- le mieux cadré ;
- le plus appétissant ;
- le plus expressif.

---

## 42. Multi-NLE

Adapters futurs :

```text
PremiereAdapter
FinalCutAdapter
FFmpegAdapter
CapCutAdapter
```

---

## 43. Marketplace

Possibilité future de partager ou vendre :

- styles ;
- templates ;
- transitions ;
- packages de primitives ;
- profils de chaîne.

---

# Résumé stratégique

## 44. Définition courte

> **AI Video App est un compilateur vidéo IA : l'utilisateur fournit un scénario, des médias et un style. L'application génère un Storyboard Exécutable, sélectionne les médias appropriés, produit un Edit Plan et compile ce plan en timeline DaVinci Resolve.**

---

## 45. Principe central

```text
IA = comprendre, raisonner, décider quoi faire.

Code déterministe = valider, calculer et exécuter précisément comment le faire.
```

---

## 46. Actif principal

L'actif principal du projet doit être :

> **Video IR / Executable Storyboard**

et non l'intégration à un logiciel particulier.

Le Video IR doit rester portable, versionné et suffisamment expressif pour devenir à terme un langage intermédiaire générique de production vidéo.

---

## 47. Priorité immédiate

Avant de construire une application complète :

1. définir le Video IR v1 ;
2. créer un POC DaVinci ;
3. tester les primitives de montage disponibles ;
4. construire un premier pipeline sur une vidéo Crossroads ;
5. mesurer le gain de temps réel ;
6. seulement ensuite élargir les capacités.

---

**Nom de travail du document :** `AI_VIDEO_APP.md`  
**Statut :** document de référence vivant du projet  
**Règle :** toute évolution majeure de l'architecture ou du Video IR doit être répercutée dans ce document.
