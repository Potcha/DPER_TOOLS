# DPM-27 — Corpus synthétique et abstention sur contours concurrents

Validation du 31 octobre 2026, basée sur `dev` après la PR #17, commit `19618bf8336bf9251858a90848770d0047eb91eb`. Cette livraison complète le corpus PDF de la PR #12 et son vérificateur de la PR #14 par des images analytiques adverses, des tests de précision et un parcours MCP réel. Les données sont entièrement synthétiques.

## Comportement livré

La recherche dirigée utilise toujours la géométrie attendue pour localiser les profils ; les pixels déterminent le contour ajusté. Après filtrage de polarité et regroupement des doublons d'un même pic, deux pics distincts dont les distances à la localisation diffèrent d'au plus **1 pixel** rendent le profil ambigu. Le contraste ne départage plus cette quasi-égalité. Un pic clairement plus proche conserve la priorité sur un voisin plus lointain, même plus contrasté.

Les profils ambigus sont exclus de l'ajustement et de sa couverture. Si le support restant est insuffisant, l'inspection retourne un message explicite et ne crée aucune mesure. Le seuil fixe de 1 pixel est une politique conservatrice liée au raster et au pas d'échantillonnage de 0,5 pixel ; ce n'est ni une incertitude calibrée ni une reconnaissance de la vérité physique. Les champs publics, les projets et les outils MCP gardent leur format actuel.

Les tests unitaires construisent les intensités à partir d'équations analytiques indépendantes du détecteur. Le parcours MCP utilise un second générateur, avec 16 échantillons de surface centrés sur chaque pixel natif, sans redimensionnement. Les valeurs attendues sont enregistrées avant toute inspection. Le nominal du PDF reste 4,000 mm, différent des diamètres observés. Les résultats incomplets restent marqués incomplets et ne deviennent une réussite globale qu'après les sept cas.

## Essais exécutés

| Contrôle | Résultat réel |
|---|---|
| Suite Rust complète | **138 réussis, 0 échec**, dont 10 nouveaux tests analytiques |
| Compilation | Réussie, 18 avertissements existants |
| Trois PDF vectoriels synthétiques | 1 / 2 / 2 cercles retrouvés, attentes et empreintes vérifiées |
| MCP sur instance dédiée, port 18765 | Parcours existant et **7 nouveaux cas réussis** |
| Isolation | Profil utilisateur inchangé ; seul le processus de test a été arrêté |

Les dix nouveaux tests couvrent les doubles contours sur lignes de plusieurs orientations, cercles de rayons 18/48/86 pixels et arcs dans les deux sens ; contrastes égaux ou inégaux ; polarité ; localisation discriminante ou presque équidistante ; reflets et saturation locaux ou généralisés ; pic unique avec plateau ; précision de petits/grands cercles uniques et angles entre deux supports bruités.

Les cercles uniques de rayons 14,7 et 93,2 pixels ont des centres non entiers, une localisation volontairement décalée et un bruit déterministe. Les erreurs de centre et de rayon doivent rester inférieures à 0,18 pixel. Les angles de 30/60/90/135 degrés sont déduits de deux supports ajustés indépendamment, sous trois rotations globales ; l'erreur doit rester inférieure à 0,2 degré. Ce test de directions n'ajoute pas un outil applicatif de mesure d'angle et ne vérifie pas les extrémités physiques des segments.

| Cas MCP | Attendu indépendant | Résultat |
|---|---:|---:|
| Disque sans défaut | Ø 4,200 mm | 4,198342 mm |
| Saturation locale traversant le bord | Ø 4,200 mm | 4,198616 mm ; couverture 92,71 % |
| Saturation généralisée dans la zone recherchée | Refus, support absent | Refus ; 0 profil détecté |
| Anneau, recherche équidistante, polarité libre | Refus, deux contours plausibles | Refus ; 96 profils ambigus |
| Même anneau, polarité montante | Bord intérieur, Ø 3,780 mm | 3,780038 mm |
| Même anneau, polarité descendante | Bord extérieur, Ø 4,620 mm | 4,619012 mm |
| Même anneau, localisation proche du bord extérieur | Ø 4,620 mm | 4,619012 mm |

Les seuils MCP, fixés avant l'essai final, sont 0,025 mm pour le diamètre et 0,35 pixel pour le centre. Les deux refus préservent la révision, la liste vide de mesures et les enregistrements vides. Chaque contexte est sauvegardé avant d'ouvrir le suivant, conformément à la protection de l'application contre la perte de modifications.

Le parcours existant continue de vérifier PDF/manifestes, distinction nominal/mesuré, constructions sans valeur, recalage indépendant de la calibration, persistance des repères, invalidation des dépendances et historique. Les contrôles d'unités/tolérances et leurs ambiguïtés sont exercés par les tests `dper_drawing` ; les décisions scalaires, y compris petites/grandes valeurs et degrés, par `dper_metrology`. Les ajustements et rejets de repères restent exercés par `dper_registration`. Ce sont des preuves logicielles distinctes, sans confusion avec une mesure physique.

Le [compte rendu machine](evidence/dpm27.json) contient les résultats, les seuils et les empreintes du contenu évalué. Les traces brutes restent dans `output/delivery/DPM-27-adversarial/` : `tests.log`, `build.log`, `vectors.log`, `smoke-04.log` et les sorties de la copie de validation. Les essais de mise au point `smoke-01` à `03` ont échoué sur le générateur/les assertions/la sauvegarde des contextes de test ; ils sont conservés et ne sont pas comptés comme réussites.

## Reproduire

Depuis le dossier `DPER_MEASURE`, avec Python/Pillow et Poppler disponibles :

```powershell
cargo test --locked
cargo build --locked
python scripts/verify_vector_fixtures.py --output-dir output/automation-validation/dpm27-vectors-nouveau
pwsh -NoProfile -File scripts/run_inspection_smoke.ps1 -SkipBuild -OutputDirectory output/automation-validation/dpm27-mcp-nouveau
```

Les dossiers de sortie doivent être nouveaux. Le lanceur refuse le port utilisateur 8765 et un port occupé ; il vérifie le propriétaire du processus et le profil utilisateur. Les fixtures, projets, PDF et traces générés restent locaux. Aucun fichier réel de pièce n'est livré.

## Limites

Ces essais qualifient des comportements logiciels sur des cas synthétiques définis, sans étalonnage physique ni garantie sur toutes les images. Un reflet unique peut rester visuellement indiscernable d'un bord réel. Le garde-fou n'est pas un détecteur général de reflets ou de saturation, et le test ne caractérise pas précisément la frontière du seuil de 1 pixel. Une localisation clairement discriminante ou une polarité peut choisir l'un des deux bords : cela n'établit pas que ce bord est celui à contrôler.

La conformité physique reste bloquée lorsque la calibration n'est pas qualifiée ou son incertitude est inconnue. Les cas MCP n'annoncent aucune pièce conforme. Les extrémités réelles, cotes 3D, recalage automatique général, acquisition optique et recette complète de l'interface demeurent hors de ce corpus (notamment DPM-22/26/28).
