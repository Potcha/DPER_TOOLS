# DPM-23 — États, groupes et détail des preuves

Base : DPM-22 fusionné, 9fd7db7d06616dafa3e53c46b3070b149929bfbc. Ce lot améliore la présentation des preuves sans modifier les algorithmes de détection ni accorder une conformité physique.

## Critères couverts

| Critère | Comportement |
| --- | --- |
| Construction, proposé, mesuré, validé logiciellement distincts | États compacts des objets dérivés d'une réévaluation courante ; aucune validation déduite d'une origine ou méthode déclarée. Aperçus distincts des mesures natives. |
| Groupes et dépendances | Historique regroupé sans perdre les observations répétées ; groupe et parents sur les fiches ; jointure mesure/record par identifiant **et capture**, pour éviter qu'un historique étranger remplace l'état ou le groupe. |
| Preuves et seuils à la demande | Points acceptés/rejetés, couverture, RMS/max, provenance, géométrie observée et réglages réellement utilisés dans les détails repliables. Les réglages sont maintenant conservés dans InspectionRecord.vision_options, optionnel pour les anciens projets ; aucun défaut rétroactif. Les seuils actuels de réévaluation sont identifiés séparément. |
| Preuve périmée et impossibilité 2D visibles | Motifs visibles même détails fermés ; valeurs périmées/importées suspendues plutôt que présentées comme actuelles. Limites d'extrémités et sélection native pour correction conservées. |
| Rapports et exports | Annexes obligatoires PDF/PPTX : groupes, objets/aperçus/constructions, preuves, réglages connus/inconnus, dépendances et blocages, même audit facultatif désactivé. Historique explicitement non réévalué. CSV/XLSX enrichis en métadonnées historiques ; constructions et valeurs périmées toujours exclues des lignes numériques. |

## Validation exécutée

- **186 tests réussis, zéro échec, un hôte ignoré par défaut** ; l'hôte MCP est ensuite exécuté explicitement avec succès. Quinze nouveaux tests couvrent les états, captures étrangères, groupes/aperçus répétés, détails par vrais clics, réglages conservés et compatibilité historique, valeur périmée masquée, exports PDF/PPTX sans audit et pagination dense, métadonnées CSV/XLSX et exclusion des valeurs périmées.
- Compilation debug réussie, 18 avertissements existants (13 dans les tests).
- Parcours MCP réel complet réussi : 18 outils, projets, dépendances/historique, PDF/manifestes et sept cas adverses ; port dédié 18766, profil inchangé, processus de test arrêté. La source utilisateur 8765 n'est pas utilisée.
- Quatre PDF produits par ce parcours : **17 pages, 3174 mots contrôlés, aucun mot hors page**. Rendus Poppler inspectés sur deux planches et la page de vue générale restante : aucun chevauchement ni coupure visible. Le rendu d'une vue générale émet un avertissement de police Symbol non disponible ; aucun glyphe visiblement manquant observé. Les tests PPTX vérifient aussi les blocs, limites de texte et annexes obligatoires.
- La première suite a signalé une attente de pagination sur une donnée qui tenait réellement sur une page. La donnée a été rendue dense en conservant l'assertion de pagination ; les journaux initiaux sont conservés. Une régression supplémentaire vérifie qu'une valeur périmée ne reste pas affichée comme actuelle.
- Relecture de coordination après les implémentations déléguées : jointures par capture et suspension des valeurs vérifiées. La revue indépendante finale par agent n'a pas pu être exécutée après épuisement du quota de ces agents ; elle n'est pas revendiquée.

[Preuve machine et empreintes](evidence/dpm23.json). Journaux et rendus locaux : output/delivery/DPM-23-evidence-ui. La branche, l'index et les changements utilisateur restent préservés ; aucun worktree créé.

## Limites

Les données sont synthétiques. Ces contrôles ne qualifient ni une pièce réelle ni une caméra/GPU, et ne remplacent pas un budget d'incertitude physique. Un état sauvegardé reste historique. Les anciens projets sans options restent lisibles et signalent l'absence de réglages conservés. Les valeurs périmées restent exclues des tableaux numériques CSV/XLSX ; les rapports PDF/PPTX et manifestes en donnent les motifs détaillés.
