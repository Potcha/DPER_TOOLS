# Risques techniques et strategie de tests

| Risque | Impact | Mitigation |
|---|---|---|
| API Resolve change selon version/licence | Execution refusee ou partielle | Sonde de capacites au demarrage, journal par version, refuse non prouve |
| Trim post-insertion absent de l'API | L'utilisateur veut corriger un in/out | Trim a l'insertion, reconstruction annoncee, future primitive a verifier |
| LLM hallucine une scene ou un media | Montage incoherent | JSON Schema + referentiel + validation semantique + fallbacks |
| Fichier deplace entre matching et execution | Plan non applicable | Re-resolution chemin, hash, alerte avant toute mutation |
| Media insuffisant | Trou narratif | Alternatives, exigences requises/recommandees, scene en attente humaine |
| Transcription erronee | Sous-titres et sens faux | Provenance, niveaux confiance, relecture UI, validation phrase |
| Job interrompu | Etat corrompu | Etapes idempotentes, checkpoints, versions immuables |
| API IA indisponible | Blocage complet | Degradation deterministe, demande explicite, brouillon non valide |
| Erreur mi-timeline | Projet Resolve alterne | Sauvegarde, journal avant/apres, rollback par restauration de timeline |

## Pyramide de tests

1. **Unites**: schema, invariants, durees, calculs frames, scores et primitives.
2. **Proprietes**: durations positives, ordre des operations, determinisme du
   compilateur et reversibilite documentaire.
3. **Contrats**: tous les providers repondent au meme schema; snapshots
   backward-compatible du Video IR.
4. **Integration**: FFmpeg reels, catalogue SQLite, jobs annules, erreurs IO.
5. **Resolve matrice**: versions/licences x primitives, dans projet isolé, avec
   media synthetique et mesures en lecture.
6. **E2E MVP**: recette de test, 10-20 rushs, verification humaine checklist.

Chaque bug Resolve reproductible devient une fixture minimaliste de capacite.
Les changements d'adapter exigent le passage du contrat Edit Plan entier, mais
pas de changement Video IR.

