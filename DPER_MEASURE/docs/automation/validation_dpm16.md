# DPM-16 — Décision conservative et base documentaire revue

Base : `43f19892553959701bb66fa81764843a57d07dea` (PR DPM-23 fusionnée). Cette tranche raccorde la règle conservative existante aux exigences confirmées et à une base indépendante fournie et revue par l'opérateur, sans propagation nouvelle de U.

## Résultat et critères

| Critère | Livraison |
| --- | --- |
| Décision distincte du contour | Une géométrie vérifiée logiciellement peut rester physiquement indéterminée. La base documentaire ne déplace pas le contour, ne change pas l'étalonnage et ne modifie pas la U native des mesures historiques. |
| Règle conservative | Valeur ± U entièrement incluse dans les limites fermées : conforme. Intervalle entièrement et strictement disjoint : hors tolérance. Chevauchement/contact extérieur : indéterminé. Sans tolérance confirmée/résolue : non évalué. |
| Qualification et U | Référence documentaire, domaine applicable, U totale élargie de cette caractéristique en mm, source documentaire de U et responsable exigés. La qualification est une déclaration externe revue ; le logiciel ne certifie pas son contenu. Ni statut du profil « verified/passed » ni U de référence/profil ne donnent automatiquement cette autorité. |
| Revue explicite et retrait | Panneau natif sur l'observation enregistrée, préparation liée au contexte puis attestation explicite. Retrait explicite possible : conformité suspendue, contour conservé. Aucun outil MCP ne donne la revue humaine ; GetState présente la même décision réévaluée. |
| Contexte et import | Preuves liées à capture/pixels, calibration complète, caractéristique, plan/source, observation, génération native et localisation (recalage/vues/associations). Révision/contexte et octets PDF vérifiés avant et après l'action native. Rechargement force imported=true et reviewed=false, même si le JSON prétend l'inverse. |
| Présentation/export | Limites, U fournie, intervalle et références sont inspectables. Changement documentaire suspend la conformité sans cacher une valeur de contour encore valable. PDF/PPTX/CSV/XLSX conservent la base comme historique non réévalué ; les notes applicatives ajoutent l'état courant réévalué. Les tableaux historiques ne sont pas promus en verdict physique courant. |

## Vérifications exécutées

- Suite finale : **205 tests réussis, zéro échec, un hôte de test ignoré par défaut** ; 19 nouveaux tests depuis la base DPM-23. L'hôte est ensuite exécuté explicitement avec succès.
- Compilation debug finale réussie ; 18 avertissements existants (13 dans les tests).
- Tests géométriques d'intégration : intervalles et contacts aux bornes, rayon/diamètre et U de la bonne caractéristique, profils historiques insuffisants, U/documents invalides, exigences non confirmées, import forgé, contexte modifié, identités ambiguës et localisation changée.
- Événements réels egui : sélection, préparation, saisies clavier, attestation, comparaison GetState, retrait, refus atomiques de contextes périmés/actions concurrentes/image vive, rechargement sans autorité. Aucun pilotage de fenêtre de bureau/GPU/caméra n'est revendiqué.
- Exports : références et U conservées dans PDF/PPTX/CSV/XLSX, même audit facultatif désactivé, sans verdict courant accordé par l'historique. Le PDF du test d'export final est identique octet pour octet au PDF rendu et inspecté.
- Parcours MCP réel sur serveur de test dédié, port 18766 : découverte de 18 outils, plan/vues/associations/recalage/contours, projets, dépendances, historique, PDF/manifestes et sept cas adverses. Serveur propriétaire arrêté et profil utilisateur inchangé ; le serveur utilisateur 8765 n'est pas utilisé. Le parcours ne simule pas une attestation humaine par MCP.
- Cinq PDF rendus et inspectés : un document de base documentaire (3 pages, 419 mots) et les quatre rapports du premier parcours MCP complet (19 pages, 3772 mots). Aucun mot hors page, aucune coupure/chevauchement visible. Avertissement Poppler existant « No display font for Symbol » sur des vues générales, sans glyphe visiblement manquant. Le parcours final répète les opérations après l'ajout des seuls contrôles natifs de retrait.
- Relectures déléguées croisées et revue de coordination : correction du masquage abusif de la valeur lors d'un changement documentaire, ajout du contexte de localisation et du retrait explicite.

[Preuve machine et empreintes](evidence/dpm16.json). Journaux, rapports synthétiques et planches locaux : `output/delivery/DPM-16-conservative-decision`. La première compilation des nouveaux tests d'export a échoué sur un type String/Cow, corrigé avant la suite finale. Les tentatives initiales du harnais MCP (accès CIM indisponible, journal encore vide et restriction réseau locale) sont conservées ; chaque processus propriétaire a été arrêté et les essais n'ont pas modifié le profil. Les parcours complets 05 puis 06 utilisent le transport local autorisé.

## Limites concrètes

Les références QUAL/U de test sont fictives. Aucune pièce, caméra ou configuration physique n'est qualifiée. Les qualifications physiques DPM-26 et propagations nouvelles DPM-15 restent séparées. Le responsable est un libellé d'attestation, pas une signature électronique authentifiée. Le panneau natif requiert actuellement un PDF source disponible. Les coordonnées et associations plan/photo restent assistées ; aucune reconnaissance universelle ni mesure d'extrémités ou de profondeur n'est ajoutée.

Les mutations de recalage/association de l'application invalident les générations d'observation avant revue ; le moteur de décision ajoute une empreinte de localisation à l'attestation. Il ne reconstitue pas une nouvelle observation par remplacement direct des structures internes. La revue ne transforme pas une preuve importée/périmée en preuve courante : réobserver, puis revoir la base. Le schéma 15 accepte le nouveau champ optionnel pour compatibilité ; son autorité est toujours réinitialisée à la désérialisation.

Branche et index utilisateur préservés. Aucun worktree créé. Seuls les fichiers DPER_MEASURE sélectionnés sont livrés ; aucun plan/photo/rapport industriel n'est publié.
