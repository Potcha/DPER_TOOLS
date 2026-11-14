# DPM-20 — Revoir et enregistrer un aperçu groupé

## Recette Windows

1. Enregistrer le projet ouvert et fermer l’ancienne instance. Depuis `O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE`, compiler avec `cargo build --locked` sur la branche de la PR ou sur `dev` après fusion. Lancer `O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE\target\debug\dper_measure.exe`.
2. Ouvrir une copie d’un projet préparé avec photo figée, plan courant, cotes circulaires associées et recalage prêt. Pour tester plusieurs occurrences, choisir une cote répétée avec au moins deux contours manquants. Les tests automatisés construisent leurs propres données synthétiques.
3. Dans **Plan de contrôle et inspection IA**, cliquer **Recommencer les aperçus (8 maximum)**. Vérifier que le nombre de mesures reste identique.
4. Dans **Avancement du contrôle**, cliquer **Examiner l’aperçu** sur la première occurrence de la page, même si une autre a été calculée après. Ouvrir **Observer les contours de la photo**. Le contour et ses preuves doivent être disponibles dans le brouillon.
5. Examiner le contour et cliquer **Enregistrer explicitement l’observation**. Vérifier qu’une seule mesure est ajoutée, avec son groupe et sa preuve. Pour la suivante, relancer les aperçus sur la nouvelle révision.
6. Refaire l’essai en modifiant un réglage de recherche dans le brouillon. Vérifier que l’enregistrement exige un nouvel aperçu compatible.

Le parcours réutilise les aperçus de la dernière page. Un enregistrement change la révision. La décision de conformité continue de dépendre des preuves et de la qualification applicables.

## Vérification logicielle

Depuis `O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE`, lancer `cargo test --locked inspection_observation_ui::tests::`. Les cas ajoutés couvrent la reprise native de la première occurrence, l’enregistrement explicite et les refus après changement du groupe, du catalogue ou des pixels. Les tests existants couvrent aussi la pagination, les brouillons et la reprise sur occurrence manquante.
