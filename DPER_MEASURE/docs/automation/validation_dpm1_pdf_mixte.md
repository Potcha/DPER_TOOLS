# DPM-1 — Interpréter un PDF mixte

Les pages avec texte vectoriel conservent leurs propositions et leurs
sources PDF. Quand une page du même PDF n’a aucun groupe de texte
vectoriel, l’application rend cette page et tente un OCR local. Chaque
cote explicite proposée conserve la page, le texte lu, la boîte sur le
rendu en pixels et la confiance OCR lorsqu’elle est disponible.

Les unités écrites dans la cote peuvent être proposées. Une unité
présente dans le cartouche d’une autre page ne complète pas une cote OCR.
Une page illisible, un OCR indisponible ou une limite de traitement
donne un avertissement. L’extension couvre les pages sans groupe de
texte vectoriel parmi les 20 premières pages, avec au plus 1 000
propositions OCR. Une page avec un cartouche vectoriel et des cotes
présentes seulement dans son image reste à relire manuellement.

## Emplacements et lancement sous Windows

- Dossier de travail `O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE`
- Exécutable debug `O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE\target\debug\dper_measure.exe`
- Client MCP `O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE\scripts\smoke_inspection_mcp.py`
- Entrées et preuves de chaque exécution dans un nouveau sous-dossier
  de `O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE\output\dpm1-pdf-mixte`

Prérequis de la recette synthétique : Python avec Pillow, Poppler
(`pdftotext`, `pdftoppm`, `pdfunite`), Tesseract 4 ou 5 avec
sortie TSV et les langues `fra` et `eng`. Vérifier la version avec
`tesseract --version`. Lancer une instance dédiée sur le port 18781.

```powershell
Set-Location 'O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE'
$env:DPER_MEASURE_MCP_PORT = '18781'
cargo run --locked
```

Dans une seconde fenêtre PowerShell, exécuter la recette suivante.
Le PDF synthétique `mixte.pdf` contient une cote vectorielle, une cote
intégrée à une image et une page vide. Le journal est dans
`rpc_evidence_001.json` et le projet de vérification dans `mixte.dperproj`.

```powershell
Set-Location 'O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE'
@'
from datetime import datetime
from pathlib import Path
import subprocess
import sys
from PIL import Image, ImageDraw, ImageFont

sys.dont_write_bytecode = True
sys.path.insert(0, 'scripts')
from generate_vector_drawing_fixtures import Drawing
from smoke_inspection_mcp import Mcp

folder = (Path('output/dpm1-pdf-mixte') /
          datetime.now().strftime('%Y%m%d-%H%M%S-%f')).resolve()
folder.mkdir(parents=True)
drawing = Drawing()
drawing.text(30, 45, 'UNITS: mm', 14)
drawing.text(30, 95, 'Longueur 32 mm', 22)
(folder / 'vectoriel.pdf').write_bytes(drawing.pdf_bytes())

scan = Image.new('RGB', (1200, 960), 'white')
font = ImageFont.truetype('C:/Windows/Fonts/arial.ttf', 82)
ImageDraw.Draw(scan).text((115, 320), 'Longueur 18 mm', font=font, fill='black')
scan.save(folder / 'scan.png')
scan.save(folder / 'scan.pdf', resolution=216)
(folder / 'vide.pdf').write_bytes(Drawing().pdf_bytes())
pdf = folder / 'mixte.pdf'
subprocess.run(['pdfunite', str(folder / 'vectoriel.pdf'), str(folder / 'scan.pdf'),
                str(folder / 'vide.pdf'), str(pdf)], check=True)
original = pdf.read_bytes()

client = Mcp(folder, 18781)
client.call('open_image', {'path': str(folder / 'scan.png')})
before = client.call('get_measurement_context')
state = client.call('analyze_drawing', {
    'path': str(pdf), 'expected_revision': before['document_revision'],
})
plan = state['inspection']['plan']
assert plan['source_fingerprint']
assert plan['extraction_method'] == 'local_pdf_bbox_layout_and_page_ocr_conservative'
vector, ocr = plan['characteristics']
assert vector['nominal'] == 32 and vector['source']['page'] == 1
assert vector['source']['pdf_bbox'] and vector['source']['word_ids']
assert ocr['nominal'] == 18 and ocr['unit'] == 'millimeter'
source = ocr['source']
assert source['page'] == 2 and source['method'] == 'pdf_page_ocr'
assert source['exact_text'] == 'Longueur 18 mm'
assert source['bbox']['width'] > 0 and source['bbox']['image_width'] > 0
assert 0 <= source['confidence'] <= 100
assert source['pdf_bbox'] is None and not source['word_ids']
assert all(not feature['confirmed'] and feature['measurement_id'] is None
           for feature in plan['characteristics'])
assert any('Page 3' in warning and 'aucune cote' in warning for warning in plan['warnings'])
after = client.call('get_measurement_context')
assert all(after[key] == before[key] for key in ('calibration', 'measurements'))
project = folder / 'mixte.dperproj'
client.call('save_project', {'path': str(project), 'overwrite': False})
client.call('open_project', {'path': str(project)})
restored = client.call('get_inspection_state')['inspection']['plan']
assert restored['source_fingerprint'] == plan['source_fingerprint']
assert restored['characteristics'] == plan['characteristics']
assert pdf.read_bytes() == original
print('Provenance et reouverture verifiees. Dossier:', folder)
'@ | python -
```

Dans **Plan de contrôle et inspection IA**, ouvrir la caractéristique
de 18 mm. Vérifier la page 2, le texte, la boîte du rendu OCR et la
confiance de lecture. La case **Interprétation du plan vérifiée**
doit rester décochée jusqu’à la revue. La cote de 32 mm conserve
ses mots sources et sa boîte PDF. La page 3 produit un avertissement.
Fermer l’instance de test après vérification.

Tests ciblés : `cargo test --locked -- dper_drawing:: reference::tests::` depuis le dossier
de travail indiqué ci-dessus.

## Résultats exécutés

- 37 tests Rust ciblés réussis, dont le scénario PDF mixte avec une
  page OCR exploitable, une page sans cote et un échec OCR.
- La recette HTTP MCP retrouve 32 mm sur la page vectorielle et 18 mm
  sur la page scannée. Les sources restent distinctes après réouverture.
  La page vide est signalée, la calibration et les mesures sont
  inchangées, et l’hôte sans fenêtre s’arrête normalement.
- Journal local dans
  `O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE\output\dpm1-pdf-mixte\20261115-170025-946707\rpc_evidence_001.json`.
- Le poste utilise Tesseract Windows 3.02, qui a échoué au premier
  passage. Le rejeu abouti utilise le moteur Tesseract 4.1.1 sous Ubuntu
  via un lanceur temporaire limité à l’hôte de test, avec
  [les données françaises officielles](https://github.com/tesseract-ocr/tessdata_fast/blob/4.1.0/fra.traineddata).
  Le rejeu avec un Tesseract Windows compatible et la vérification
  de la fenêtre restent à faire.
