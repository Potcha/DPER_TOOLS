# Validation DPM-29 — restauration du ruban

Date : 5 novembre 2026. Base dev : `74bb9d6b9845ed4f272fac342528cc87da6ed140`.
Branche : `codex/DPM-29-restore-solidcraft-ribbon`.

## Résultats

- Suite Rust : **289 réussites, zéro échec, un hôte MCP ignoré par défaut**.
  L'hôte est ensuite exécuté explicitement par la recette complète.
- Sept contrôles du ruban : neuf cellules distinctes, protections du tracé et de
  la calibration, vrais clics sur onglet/menu/sous-menu Cercles, accès via Plus
  en fenêtre de 380 points, commandes désactivées et gel du flux par Sélection.
- Compilation debug réussie ; 46 avertissements, dont composants historiques
  restaurés encore inutilisés et repli du cache de compilation. Ce nombre n'est
  pas présenté comme une dette exclusivement préexistante.
- Parcours MCP réel : 18 outils découverts, sept cas adverses, sauvegarde/reprise,
  invalidation des dépendances, historique et exports réussis.
- PDF : 19 cas d'attache, 11 cas d'unités et 17 cas de pointes réussis.
- Profil utilisateur inchangé, hôte de test 18766 arrêté, serveur utilisateur
  8765 non utilisé. Aucun test physique ou caméra n'est revendiqué.
- Deux tests Python : verrouillage transitoire/persistant du journal. Seule la
  publication du fichier est retentée ; l'appel métier n'est jamais rejoué.
- Treize PNG contrôlés par SHA-256 et comparaison octet à octet à la sauvegarde
  antérieure à la synchronisation : identité confirmée.

## Rendu et revue

Le mode explicite `--ui-review` ouvre l'application eframe réelle avec un profil
APPDATA isolé et une image synthétique. Cinq captures natives ont été produites :
Caméra, Mesures, Calibration active, Inspection et Image. Les captures sont
inspectées pour contrôler les neuf icônes, les dimensions et contrastes, la grande
visionneuse, le panneau droit, et l'ordre Longueur connue / Valider / Annuler.
Elles ne testent pas un document industriel ni un contrôle physique.

La revue indépendante a relevé deux régressions, corrigées avant livraison :
Sélection devait figer le flux, et l'annulation du choix d'un repère devait rester
cliquable lorsque le panneau de référence est affiché à droite. La relecture
finale n'a trouvé aucun blocage de livraison.

## Incidents conservés dans les journaux

Un premier essai des clics a échoué car la famille de police Semibold n'était pas
initialisée dans l'application de test ; l'initialisation commune a été corrigée.
Une recette intermédiaire a échoué lors de la publication du journal RPC
(PermissionError Windows) ; les tentatives bornées de publication sont testées
sans rejeu d'appel. Un lancement sans accès réseau local a été refusé par
l'environnement ; les recettes autorisées ont ensuite réussi. Les journaux
intermédiaires restent locaux, sans les présenter comme des succès.

## Reproduction et réception

Dans `O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE` :

```powershell
cargo test --locked ribbon
python -m unittest scripts.test_rpc_journal
./scripts/test_inspection_workflow.ps1 -OutputDirectory output/recette-dpm29-nouvelle
cargo run --locked
```

Le dossier de recette doit être nouveau. Pour la réception visuelle, suivre les
six étapes du document `docs/DPER_MEASURE_ruban_SolidCraft_adaptation.md`.
Les résultats détaillés, empreintes des sources et chemins locaux des journaux
sont dans `docs/automation/evidence/dpm29.json`. Les captures, exécutables et
rapports synthétiques restent exclus de Git sous `output/` et `target/`.

Cette livraison restaure le ruban et le cadre de travail. Les compléments
fonctionnels de constructions et motifs suivis par DPM-30 restent distincts.
