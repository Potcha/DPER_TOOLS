# DPM-4 · Observer les occurrences sur la photo

Chaque cercle affecté dans une vue PDF peut maintenant être observé séparément sur l’image recalée. Le choix du numéro d’occurrence détermine la recherche sur les pixels. L’enregistrement exige l’aperçu immédiatement précédent de la même occurrence, avec les mêmes réglages, sources et révision.

Le cercle ajusté et les points de contour proviennent de la photo. Les zones de recherche de deux occurrences doivent être disjointes. L’affectation conserve les identifiants du membre, du candidat, de la vue et l’empreinte du groupe. Une nouvelle affectation suspend les anciennes mesures et leurs descendants, même si l’ordre initial est ensuite restauré.

Le compteur représente les occurrences distinctes avec un contour courant vérifié. Pour décider de la conformité de la cote répétée, DPER_MEASURE exige une seule observation courante par emplacement, dans la même capture et le même groupe de contrôle. Chaque occurrence exige une revue de la correspondance plan/photo, une qualification indépendante et une incertitude élargie totale. La décision du groupe est conforme si toutes les occurrences sont conformes, hors tolérance si au moins une occurrence est hors tolérance et que toutes ont une décision déterminée. Une occurrence manquante, ambiguë ou indéterminée suspend la décision. Les références géométriques non résolues restent à traiter dans DPM-4.

## Validation dans l’interface

1. Importer le plan et une photo calibrée. Analyser les géométries PDF, définir la vue, puis affecter ses occurrences comme dans la procédure précédente. Établir le recalage dans cette vue avec au moins trois repères.
2. Dans **Observer les contours de la photo**, choisir la cote, puis une occurrence. Renseigner le groupe et cliquer sur **Préparer une nouvelle observation**.
3. Cliquer sur **Prévisualiser les contours**. Examiner les points acceptés, la couverture, les résidus et la localisation sur la photo. Cliquer ensuite sur **Enregistrer explicitement l’observation**.
4. Répéter pour les autres occurrences. Vérifier les numéros dans les libellés des mesures et le compteur sous **Répétitions et relations du plan**.
5. Pour une cote confirmée, ouvrir **Base documentaire de décision de conformité**. Pour chaque occurrence, vérifier l’emplacement indiqué, renseigner la qualification et l’incertitude totale déjà établies, puis attester la revue. Vérifier le résultat du groupe sous **Répétitions et relations du plan** ou dans `drawing_occurrence_observations` via MCP. Une deuxième observation courante du même emplacement suspend le résultat jusqu’à résolution du doublon.
6. Enregistrer le projet et exporter le PDF. Les observations, valeurs et décisions individuelles sont présentées comme des données historiques dans le rapport. Le verdict courant du groupe se consulte dans la session. La réouverture conserve la trace et demande de renouveler les observations courantes.

Le projet écrit au schéma 20 pour conserver la nouvelle preuve. La lecture des projets au schéma 19 reste prise en charge.

## Recette synthétique exécutée

Le scénario reproductible est défini dans `scripts/verify_occurrences_mcp.py`. Il construit deux disques dont les diamètres analytiques sont 3,500 et 3,220 mm avec un étalonnage indépendant de 0,035 mm/px. Le plan indique deux occurrences de 3,360 mm, dont la moyenne coïncide volontairement avec le nominal.

Les observations MCP donnent respectivement **3,507436 mm et 3,227360 mm**. Chaque preuve contient 96 points acceptés appartenant à son propre contour. L’écart maximal au diamètre analytique est inférieur à 0,008 mm sur cette rasterisation synthétique. La conformité du groupe reste indéterminée.

Le scénario couvre les refus pour occurrence absente ou inconnue, recalage hors de la vue, enregistrement direct et aperçu d’un autre membre. Il vérifie le projet, le PDF, le manifeste et l’invalidation des deux mesures et du descendant après réaffectation. Les six pages du PDF ont été rendues et inspectées. Poppler signale l’avertissement existant sur la police Symbol lors du rendu.

Avant la demande de limiter les suites, 326 tests Rust et la recette complète ont réussi, avec compilation debug et hôte MCP dédié. Le serveur de test a été arrêté et le profil utilisateur est resté identique. Les trois nouveaux tests d’occurrence et le test de rapport concerné ont ensuite réussi sur le contenu final. Ces essais qualifient le comportement logiciel sur données synthétiques.

Les journaux locaux sont dans `output/validation_dpm4_observations_20261107_b/`. Le PDF d’exemple se trouve dans `mcp/occurrence-checks/observed_occurrences.pdf`. Les dernières vérifications ciblées figurent dans `output/dpm4_occurrence_final.log` et `output/dpm4_occurrence_report_final.log`.
