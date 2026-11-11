# DPM-4 · Références de repères dans un PDF

Une déclaration textuelle autonome `DATUM A` ou `REPÈRE A` est indexée avec sa page, sa boîte et ses mots PDF. Une ligne géométrique du type `⌖ | Ø0,1 mm | A | B` conserve l’ordre de ses repères et la provenance de la cote. Une déclaration unique de même lettre sur la même page fournit un candidat dont la vue et le support physique restent à établir. Les doublons, lettres isolées, déclarations sur une autre page et cadres difficiles à interpréter maintiennent la suspension du contrôle.

Le résultat est recalculé depuis les sources textuelles conservées dans le plan, y compris après réouverture. Le panneau **Répétitions et relations du plan**, `get_inspection_state` et les notes de structure du rapport présentent ces éléments. Les repères encadrés qui n’ont pas de déclaration textuelle explicite, les modificateurs et les références composées restent à traiter. La confirmation de la lecture de la cote conserve le blocage métrologique.

## Recette dans l’application

Depuis `O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE`, générer les deux plans synthétiques dans un nouveau dossier, puis lancer l’application de la branche de la PR.

```powershell
python .\scripts\verify_drawing_datums_mcp.py .\output\dpm4-reperes-recette
cargo run --locked
```

1. Importer `O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE\output\dpm4-reperes-recette\repere-unique.pdf` comme plan de référence. Ouvrir **Plan de référence → Plan de contrôle et inspection IA → Répétitions et relations du plan**.
2. Vérifier la déclaration `DATUM A` en page 1 avec sa boîte en points PDF et ses identifiants de mots. Le lien vers A doit demander de vérifier la vue et le support. B doit être signalé comme absent, même si la lettre `B` apparaît seule sur le plan.
3. Importer `O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE\output\dpm4-reperes-recette\repere-duplique.pdf`. Vérifier les deux déclarations de A et l’ambiguïté du lien. Le contrôle doit rester suspendu dans les deux cas et le nombre de mesures photo doit rester identique.

Avec un hôte MCP de test dédié, ajouter `--test-port <port>` à la commande Python pour vérifier les mêmes cas automatiquement. Le dossier de sortie doit être neuf.

## Résultats exécutés

- `cargo test --locked drawing_structure::` · 7 réussis, dont 3 nouveaux, zéro échec et 456 filtrés.
- Transport MCP réel sur hôte dédié · 2 PDF extraits par Poppler. Provenance, candidat à vérifier, doublon, cible manquante et mesures photo inchangées vérifiés. L’hôte a été arrêté normalement.
- La manipulation du panneau Windows et le rendu du rapport restent à vérifier. Le code du rapport réutilise les mêmes lignes de synthèse que le panneau. La suite complète est réservée au bilan final.
