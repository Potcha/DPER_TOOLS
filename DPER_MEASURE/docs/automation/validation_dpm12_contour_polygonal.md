# DPM-12 — Contour polygonal guidé

`inspect_polygon_contour` examine, sur la capture figée, un contour convexe
unique décrit par trois à huit sommets dans l’ordre du pourtour. Les sommets
du guide délimitent les bandes de recherche. Les droites sont ajustées aux
pixels et leurs intersections donnent les sommets restitués.

Chaque côté du guide mesure entre 14 et 516 px. Les profils sont espacés
d’au plus 2 px entre deux retraits de 3 px aux extrémités. Le rayon de
recherche est compris entre 1 et 32 px. Un profil manquant, deux pics
distincts au-dessus du seuil ou un point écarté par l’ajustement font
refuser le contour. Les intersections doivent être dans l’image, à 4 px
au plus des deux extrémités observées et formées par des supports dont
l’angle aigu atteint au moins 15°. Des profils à 3, 5 et 7 px au-delà de
chaque sommet vérifient l’absence locale de prolongement.

Le résultat en pixels contient les supports, les échantillons et les
résidus, la capture, sa révision et l’empreinte de l’image. Le diagnostic
est en lecture seule. Sa portée est limitée au contour décrit et aux
profils sondés. Les assemblages, contours concaves ou courbes, et la
qualification physique restent hors du domaine de cette opération.

## Démarrage sous Windows

- Dossier de travail `O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE`
- Exécutable debug `O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE\target\debug\dper_measure.exe`
- Client MCP `O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE\scripts\smoke_inspection_mcp.py`
- Images générées et journal `O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE\output\dpm12-polygon-mcp`

Depuis la branche qui contient DPM-12, lancer une instance dédiée
sur le port 18778. Le rejeu ouvre les images synthétiques dans cette
instance. Python 3 avec Pillow est requis.

```powershell
Set-Location 'O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE'
$env:DPER_MEASURE_MCP_PORT = '18778'
cargo run --locked
```

Dans une seconde fenêtre PowerShell, exécuter le bloc ci-dessous, puis
fermer l’instance dédiée. Le dossier de sortie reçoit les quatre PNG et le
journal `rpc_evidence_001.json` au premier passage.

```powershell
Set-Location 'O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE'
@'
import math
from pathlib import Path
import sys
from PIL import Image

sys.dont_write_bytecode = True
sys.path.insert(0, 'scripts')
from smoke_inspection_mcp import Mcp

folder = Path('output/dpm12-polygon-mcp').resolve()
folder.mkdir(parents=True, exist_ok=True)
trapezoid = [(40.5, 45.5), (145.5, 45.5), (130.5, 125.5), (55.5, 125.5)]
rectangle = [(40.5, 45.5), (145.5, 45.5), (145.5, 125.5), (40.5, 125.5)]

def fixture(name, points):
    edges = []
    for (x, y), (u, v) in zip(points, points[1:] + points[:1]):
        edges.append((x, y, (u-x)/math.hypot(u-x, v-y), (v-y)/math.hypot(u-x, v-y)))
    pixels = bytearray()
    for y in range(180):
        for x in range(185):
            distance = min(dx*(y-v) - dy*(x-u) for u, v, dx, dy in edges)
            if name == 'prolongement' and x <= 35:
                distance = y - 45.5
            value = round(125 + 95 * math.tanh(distance / 0.7))
            if 85 <= x <= 95 and (
                (name == 'masque' and 37 <= y <= 54) or
                (name == 'reflet' and 49 <= y <= 51)
            ):
                value = 30
            pixels.append(value)
    path = folder / (name + '.png')
    Image.frombytes('L', (185, 180), bytes(pixels)).save(path)
    return path

client = Mcp(folder, 18778)

def state():
    return (client.call('get_measurement_context'), client.call('get_inspection_state'))

def open_fixture(name, points):
    client.call('open_image', {'path': str(fixture(name, points))})
    return state()

def inspect(baseline, points, radius, error=None, stale=False):
    context = baseline[0]
    result = client.call('inspect_polygon_contour', {
        'capture_id': context['capture_id'],
        'expected_revision': context['document_revision'] - int(stale),
        'guide': [dict(x=x, y=y) for x, y in points],
        'search_radius_px': radius,
    }, expect_error=error is not None)
    assert state() == baseline
    if error:
        assert error in result['content'][0]['text'].lower(), result
        return
    assert result['capture_id'] == context['capture_id']
    assert result['document_revision'] == context['document_revision']
    assert len(result['image_fingerprint']) == 16
    return result['diagnostic']

initial = open_fixture('trapeze', trapezoid)
guide = [(x + 0.75, y) for x, y in trapezoid]
result = inspect(initial, guide, 4)
errors = [math.hypot(corner['intersection_px']['x']-x, corner['intersection_px']['y']-y)
          for corner, (x, y) in zip(result['corners'], trapezoid)]
assert len(result['corners']) == len(result['sides']) == 4
assert max(errors) < 0.2
assert all(side['evidence']['coverage_fraction'] == 1 for side in result['sides'])
inspect(initial, [guide[i] for i in (0, 2, 1, 3)], 4, 'convexes')
inspect(initial, guide, 4, 'vision', stale=True)
inspect(open_fixture('masque', rectangle), rectangle, 7, 'manquants')
inspect(open_fixture('reflet', rectangle), rectangle, 7, 'ambigus')
inspect(open_fixture('prolongement', rectangle), rectangle, 4, 'se prolonge')
print('6 cas MCP verifies. Erreur maximale des sommets en px:', max(errors))
print('Journal:', client.log_path)
'@ | python -
```

Le trapèze est restitué avec quatre côtés et quatre intersections à moins
de 0,2 px de la référence synthétique, malgré un guide décalé de 0,75 px.
Le guide croisé, la révision périmée, le masquage, le reflet et le
prolongement donnent chacun un refus. Après chaque diagnostic, les états
de mesure et d’inspection doivent correspondre à ceux lus juste avant.

## Résultats exécutés

- 19 tests Rust ciblés de vision réussis, dont deux pour le contour polygonal.
- Six cas réussis par HTTP MCP sur l’hôte dédié sans fenêtre. Écart maximal
  mesuré de 0,0044 px sur le trapèze synthétique. Arrêt normal de l’hôte.
- Journal local `O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE\output\dpm12-polygon-mcp\rpc_evidence_001.json`.
