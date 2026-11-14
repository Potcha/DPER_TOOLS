# DPM-20 — validation de la reprise des aperçus

Depuis `O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE`, exécuter :

```powershell
& 'C:\Users\miche\.cargo\bin\cargo.exe' test --locked batch_preview_ -- --nocapture *> 'O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE\output\dpm20-resumable-tests.log'
```

Résultat observé : 8 tests réussis, aucun échec ; 496 tests non sélectionnés, 30 avertissements de compilation. Journal : `O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE\output\dpm20-resumable-tests.log`.

Dans l'interface, démarrer avec « Recommencer les aperçus », puis choisir « Continuer les aperçus » tant que disponible. Chaque page tente au plus huit recherches ; les erreurs restent visibles et ne bloquent pas la page suivante. Une reprise invalide ou périmée est refusée : recommencer. Un brouillon natif ouvert est conservé et empêche l'opération.

Pour MCP, omettre `cursor` pour démarrer, puis transmettre `next_cursor`, `capture_id` et `document_revision` retournés à chaque appel. Un curseur déjà utilisé ou associé à un contexte modifié est refusé. `next_cursor` vaut null à la fin. Les aperçus ajoutent des preuves, sans créer de mesure ni de décision de conformité. La validation physique et la vérification manuelle de l'interface ne sont pas couvertes par ces tests.

Application à lancer pour la vérification manuelle : `O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE\target\debug\dper_measure.exe` (à construire séparément si nécessaire).

## Recette manuelle

1. Enregistrer le projet utilisateur avant de fermer l'ancienne instance. Lancer l'exécutable ci-dessus compilé sur `codex/DPM-20-resumable-previews`, ou sur `dev` après fusion. L'installateur précédent ne contient pas cette évolution.
2. Ouvrir une copie d'un projet préparé avec photo figée, plan PDF courant, associations circulaires et recalage prêts, et plus de huit occurrences manquantes. Les fichiers de préparation restent ceux du projet utilisateur ; aucun plan industriel n'est livré avec ce test. Noter le nombre de mesures avant l'essai.
3. Dans « Plan de contrôle et inspection IA », cliquer « Recommencer les aperçus (8 maximum) ». Vérifier huit recherches au maximum, leurs preuves et les recherches en attente.
4. Cliquer « Continuer les aperçus (8 maximum) ». Vérifier que les identifiants d'occurrences correspondent aux recherches suivantes, sans répétition des premières. Poursuivre jusqu'à disparition du bouton de continuation. Le nombre de mesures doit rester identique.
5. Avec un contour absent, vérifier que son erreur est indiquée et que la continuation atteint les autres occurrences. Pour refaire les recherches ayant échoué, choisir explicitement « Recommencer ».
6. Entre deux pages, modifier une mesure ou un réglage : la continuation doit être refusée et demander un redémarrage. Un brouillon natif ouvert doit empêcher l'opération sans être remplacé.
7. Préparer individuellement une occurrence, prévisualiser puis enregistrer après revue des pixels : la pagination ne remplace pas cette étape de confirmation.

La continuation est temporaire et propre à cette instance : fermer l'application ou redémarrer les aperçus invalide le jeton précédent. La réponse de chaque page décrit ses résultats et les recherches restantes ; elle ne cumule pas les messages des pages précédentes. Les preuves restent dans la session. Diamètres/rayons uniquement ; les autres familles restent visibles dans l'avancement.

## Vérifications de livraison

Compilation `cargo build --locked` réussie en 23,30 secondes avec 39 avertissements de code inutilisé ; journal `O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE\output\dpm20-resumable-build.log`. Relecture du code et `git diff --check` réussis. Aucun nouveau test HTTP MCP, aucune recette interactive de la fenêtre Windows ni suite complète exécutés pour ce lot. Les tests ciblés passent par l'opération applicative et son adaptateur de requête ; le changement des pixels sans changement de révision est couvert. DPM-20 reste ouvert pour la suite du parcours complet.
