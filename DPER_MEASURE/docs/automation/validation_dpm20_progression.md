# DPM-20 — Vérifier l'avancement du contrôle

Cette étape expose un état de lecture commun à l'interface et à `get_inspection_state`. Elle ne lance pas une inspection automatique complète.

1. Lancer la version construite depuis la branche de cette livraison. Importer un plan contenant plusieurs cotes, dont une sans tolérance et, si possible, une cote répétée.
2. Dans « Plan de contrôle et inspection IA », ouvrir « Avancement du contrôle ». Vérifier une ligne par caractéristique, y compris celles sans observation. Vérifier le nombre attendu, la conformité et les raisons affichées.
3. Préparer un aperçu de contour sans l'enregistrer : il ne doit pas être compté comme une observation courante vérifiée. Enregistrer ensuite l'observation et vérifier la mise à jour.
4. Enregistrer le projet, le fermer puis le rouvrir. Une preuve historique ne doit pas être comptée comme observation courante sans réévaluation ; le compteur historique doit la signaler.
5. Demander au client MCP `get_inspection_state` et comparer `characteristic_progress` avec le panneau. Les coordonnées et valeurs nominales du plan ne sont pas des mesures de la photo.

Les résultats de conformité dépendent des contrôles existants et d'une qualification physique indépendante. Les données synthétiques ne qualifient pas la chaîne optique. L'orchestration complète, le traitement de tous les types de plans et les rapports de synthèse restent à achever dans DPM-20.
