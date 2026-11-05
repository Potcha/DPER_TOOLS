# Revue commune et reprise — 5 novembre 2026

## Base et périmètre
Base examinée : dev, commit fa7e6dcdd30a63d8d511464d25c6de70f7360348 (PR #31).
La copie de travail a été synchronisée dans le dépôt existant, sans nouveau worktree. Son ancien code, ses documents et données sont conservés dans une archive vérifiée et dans la réserve Git 562609f823e5897905910c0088e4b194ebf6b2f5.
Cette revue rapproche les tickets/PR par famille et vérifie leurs interactions ; elle ne prouve pas l'absence de tout défaut ni une aptitude physique.

## Matrice des interactions livrées
| Tickets / PR | Interaction revue et contrôlée | Résultat / limite |
|---|---|---|
| DPM-2, 33–39 / #9,14,26–31 | Texte PDF, géométries, vues, unités, hypothèses et fraîcheur des sources | Alternatives et abstention conservées ; trois tests de #31 étaient imbriqués et non exécutés, corrigé DPM-41 |
| DPM-6 / #10,17 | Recalage, correspondances, sauvegarde et calibration indépendante | Parcours MCP commun réussi, repères/résidus conservés |
| DPM-10,27 / #11,12,18 | Contours réels, ambiguïtés, qualité, preuve client forgée | Sept cas adverses réussis ; aucun nominal utilisé pour imposer le diamètre observé |
| DPM-14,16 / #8,24 | Géométrie observée, exigences, attestations et incertitude | Matrice native et rejeu indépendant réussis ; aucune qualification réelle déduite |
| DPM-18,19,38 / #15,16,30 | Modification PDF, observation sans vue/association, descendants et exports | Défaut confirmé puis corrigé DPM-40 ; export direct et dialogue dossier protégés |
| DPM-22,23 / #19–23 | Parcours natif, sélection, groupes, projets et rapports | Recette native composée et réseau réussies ; bureau/GPU non qualifiés |
| DPM-28 / #25 | Recette composée et conservation des journaux | Écriture directe des journaux interrompue sur Windows ; publication atomique ajoutée DPM-41 |

## Corrections de consolidation
**DPM-40** : une source PDF remplacée suspend désormais les descendants d'une observation explicite même sans association ni vue de recalage. L'export direct calcule une copie conservatrice sans modifier l'application. Pour un export dossier déjà ouvert, les PDF avec empreinte sont revérifiés avant génération et après le choix de destination ; une source modifiée/absente provoque un refus explicite. Les mesures manuelles indépendantes, les imports historiques et les plans textuels sans fichier conservent leurs règles existantes.

**DPM-41** : les trois tests de conflits d'unités de PR #31 sont au niveau du module et exécutés. Le journal MCP est écrit dans un fichier temporaire du même dossier puis publié par remplacement atomique. Un échec persistant reste une erreur ; aucun appel métier n'est rejoué pour réécrire le journal.

Les dossiers output/ et dist/ sont exclus des nouveaux ajouts Git pour garder les résultats locaux et les installateurs hors des PR.

## Résultats réellement obtenus
Recette finale : scripts/test_inspection_workflow.ps1, résultats locaux output/revue-dev-04/.
- 282 tests réussis, zéro échec ; un hôte ignoré par défaut puis démarré explicitement par le runner.
- Compilation debug réussie, avertissements de code inutilisé conservés.
- Transport réel des 18 outils MCP ; sept cas adverses réussis.
- 19 cas de renvois, 11 cas d'unités et 17 cas de pointes PDF réussis.
- Sauvegarde/réouverture, invalidation, exports PDF/manifestes et parité UI/MCP exercés.
- Serveur de test 18766 arrêté, profil utilisateur inchangé ; serveur utilisateur 8765 non utilisé.
- Relecture indépendante des corrections, sans nouveau blocage concret relevé.
- Preuve machine et empreintes : [consolidation_20261105.json](evidence/consolidation_20261105.json).

Essais intermédiaires conservés dans output/ : la première suite confinée échouait sur deux écritures temporaires (OS650) ; reprise avec accès normal 276/0. Les essais revue-dev-02/03 ont été interrompus par OSError22 pendant la réécriture du journal MCP. Les nouvelles régressions de dépendances et d'export ont échoué avant correction puis réussi après. Ces essais ne sont pas présentés comme une recette complète réussie.

## Reprise des cinq lots demandés
| Ticket | État technique constaté | Livraison suivante et validation |
|---|---|---|
| DPM-4 | Des champs de relation et des libellés de groupe existent, mais pas de résolution métier générale ni de groupe à membres attendus | Construire modèle explicite, sources/justifications, statuts et invalidation des relations non résolues ; tests de compatibilité, UI/MCP, projet et exports |
| DPM-29 | Ruban et PNG fournis conservés dans la sauvegarde ; modules non intégrés dans dev | Reprendre images et contrôles natifs, raccorder commandes aux actions actuelles ; vérifier accès aux panneaux d'inspection et alignement/contraste en application |
| DPM-30 | Variantes, motifs, sélection et graphismes locaux non livrés | Intégrer sur modèle actuel is_construction/source_measurement_ids/dependency_stale ; tester centrage, toutes rangées, identifiants, édition, undo/redo, rechargement et exports |
| DPM-31 | Installateur n'embarque pas tous les outils externes ; cible x64 par défaut et documentation dépôt à clarifier | Garantir cible Windows x64, détecter/documenter ou distribuer Poppler (dont pdftocairo), Tesseract+fra/eng et FFmpeg ; recette installation/lancement/mise à niveau/désinstallation |
| DPM-26 | Protocole récupéré, campagne physique non réalisée | Exécuter [le protocole indépendant](protocole_qualification_physique_dpm26.md) : référence certifiée distincte, champ/orientations/répétitions, biais, budget U et revue du domaine |

Les absences DPM-29/30 dans dev sont des fonctions restées locales, pas une preuve qu'une PR ultérieure les aurait supprimées.
L'incertitude propagée générale, l'appariement automatique et la lecture universelle de tout plan restent des chantiers ouverts. DPM-26 ne peut pas être terminé par un test synthétique.

## Ordre et circuit
1. Revoir et fusionner la PR de consolidation DPM-40/41 vers dev.
2. Synchroniser le même dossier sur dev et enregistrer le commit de base.
3. Traiter DPM-4 puis DPM-29/30, avec PR distinctes et recette commune après chaque intégration ; conserver la qualification physique séparée.
4. Stabiliser DPM-31 sur l'application consolidée et conduire DPM-26 sur le matériel.
5. Pour chaque ticket : critère → code → commit testé → preuve → PR → À valider → validation utilisateur/fusion → Terminé.

La synchronisation ne doit pas appliquer globalement l'ancienne réserve : elle réintroduirait les versions antérieures de fichiers partagés. Reprendre uniquement les fonctions du lot et conserver les nouvelles protections.
