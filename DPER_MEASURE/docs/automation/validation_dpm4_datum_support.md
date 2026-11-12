# DPM-4 — Proposition documentaire de support d’un repère

`inspect_drawing_datum_support` examine le lien explicitement proposé entre une référence de repère et un support vectoriel dans une vue PDF courante. Il restitue les sources de la cote, de la déclaration et du support, l’empreinte du document et la justification de l’opérateur. Il accepte une ligne droite ou un cercle candidat.

Le résultat reste une proposition en lecture seule. Il ne prouve pas le rôle fonctionnel du support, ne confirme pas le repère et ne crée aucune mesure photo. Le contrôle dépendant reste suspendu avec `drawing_relation_unresolved`. La persistance du lien et sa revue explicite restent à développer dans DPM-4.

## Recette immédiate

Enregistrer le travail ouvert avant d’importer les fichiers de recette. L’exécutable compilé pour cette livraison est `O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE\target\debug\dper_measure.exe`. L’installateur existant ne contient pas ce changement.

Les trois plans synthétiques utilisés pour la vérification sont disponibles localement :

- `O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE\output\dpm4-datum-support-20261112-231027\unique.pdf`
- `O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE\output\dpm4-datum-support-20261112-231027\duplicate.pdf`
- `O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE\output\dpm4-datum-support-20261112-231027\outside.pdf`

1. Lancer cet exécutable et connecter le client MCP. Aucune image ou calibration n’est nécessaire.
2. Demander au client d’analyser `unique.pdf` avec `analyze_drawing`, puis sa page 1 avec `analyze_drawing_geometry`. Lire les identifiants depuis `get_inspection_state` et `get_drawing_geometries`. Ne pas réutiliser des identifiants d’une session précédente.
3. Définir une vue explicite avec `define_drawing_view`, contenant la première caractéristique géométrique et la ligne droite. La boîte de cette vue doit inclure la déclaration `DATUM A`.
4. Lire et conserver `get_inspection_state` et `get_measurement_context`. Appeler `inspect_drawing_datum_support` avec la révision courante, les identifiants `feature_id`, `view_id`, `support_id`, `label: "A"` et une `justification` non vide.
5. Attendre `state: "documentary_proposal"`, la déclaration `DATUM A`, le support choisi et les sources complètes. Les champs `reviewed`, `reference_resolved` et `measured_on_photo` doivent être faux. La suspension doit rester `drawing_relation_unresolved`.
6. Répéter avec `label: "B"`, un support absent, une justification vide, une ancienne révision, puis un champ client supplémentaire `reviewed: true`. Les cinq appels doivent être refusés.
7. Répéter la préparation sur `duplicate.pdf`, puis sur `outside.pdf`, avec les nouveaux identifiants et révisions. Le doublon A doit être refusé. Sur `outside.pdf`, définir la même vue cote/ligne que précédemment, sans l’élargir à la déclaration éloignée. La déclaration hors vue doit être refusée.
8. Après chaque diagnostic accepté ou refusé, comparer les deux états conservés à l’étape 4. Ils doivent rester identiques. Aucun nouveau tracé n’est attendu.

L’interface Windows ne propose pas encore de commande native pour ce nouveau diagnostic. La recette passe par le client MCP.

## Reproduction automatisée

Le script `O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE\scripts\verify_datum_support_mcp.py` génère les trois PDF et vérifie les huit cas ci-dessus. Il requiert un hôte MCP de test propriétaire déjà démarré, sur un port dédié supérieur ou égal à 18765. Il ne doit pas être dirigé vers la session utilisateur sur 8765. Le dossier de sortie doit être neuf.

```powershell
Set-Location 'O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE'
python .\scripts\verify_datum_support_mcp.py .\output\dpm4-datum-support-nouvelle-recette --test-port 18776
```

## Vérifications exécutées

- `cargo test --locked drawing_structure::` : huit tests réussis, zéro échec, 460 filtrés. Le nouveau test couvre les sources, le support hors vue, la déclaration hors vue, le doublon et la source modifiée.
- Transport MCP réel sur un hôte de test dédié au port 18776 avec extraction PDF par Poppler : huit cas réussis, dont une proposition et sept refus. Les états complets de mesure et d’inspection restent identiques après les diagnostics. Hôte arrêté normalement.
- `cargo build --locked` réussi avec 40 avertissements de code inutilisé.
- `git diff --check` réussi. Aucune suite complète relancée et aucun nouvel installateur produit.

Les journaux, PDF et réponses RPC restent locaux dans `output/`. La synthèse est `O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE\output\dpm4-datum-support-20261112-231027\summary.json`. Ces essais logiciels synthétiques ne qualifient aucune pièce réelle.
