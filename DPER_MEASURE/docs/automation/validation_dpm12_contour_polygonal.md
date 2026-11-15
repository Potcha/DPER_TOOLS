# DPM-12 — Contour polygonal guidé

`inspect_polygon_contour` examine, sur la capture figée, un contour simple, convexe ou concave,
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

Le guide et les intersections observées doivent former un polygone simple.
Les côtés croisés, les contacts entre côtés non adjacents, les sommets répétés
et les sommets alignés sont refusés. Chaque angle observé conserve le sens
du virage du guide, y compris les angles rentrants.

Le résultat en pixels contient les supports, les échantillons et les
résidus, la capture, sa révision et l’empreinte de l’image. Le diagnostic
est en lecture seule. Sa portée est limitée au contour décrit et aux
profils sondés. Les assemblages, contours courbes, et la
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
concave_l = [(40.5,40.5), (180.5,40.5), (180.5,90.5),
             (90.5,90.5), (90.5,180.5), (40.5,180.5)]
concave_u = [(40.5,40.5), (80.5,40.5), (80.5,140.5), (140.5,140.5),
             (140.5,40.5), (180.5,40.5), (180.5,180.5), (40.5,180.5)]

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

def concave_fixture(name):
    def rect(x, y, left, top, right, bottom):
        return min(x-left, right-x, y-top, bottom-y)
    pixels = bytearray()
    for y in range(220):
        for x in range(220):
            if name == 'concave_u':
                distance = max(rect(x,y,40.5,40.5,80.5,180.5),
                               rect(x,y,140.5,40.5,180.5,180.5),
                               rect(x,y,40.5,140.5,180.5,180.5))
            else:
                distance = max(rect(x,y,40.5,40.5,180.5,90.5),
                               rect(x,y,40.5,40.5,90.5,180.5))
            value = round(125 + 95 * math.tanh(distance / 0.7))
            if 120 <= x <= 130 and (
                (name == 'concave_masque' and 82 <= y <= 99) or
                (name == 'concave_reflet' and 86 <= y <= 88)
            ):
                value = 30
            pixels.append(value)
    path = folder / (name + '.png')
    Image.frombytes('L', (220,220), bytes(pixels)).save(path)
    return path

client = Mcp(folder, 18778)

def state():
    return (client.call('get_measurement_context'), client.call('get_inspection_state'))

def open_fixture(name, points):
    path = concave_fixture(name) if name.startswith('concave_') else fixture(name, points)
    client.call('open_image', {'path': str(path)})
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
inspect(initial, [guide[i] for i in (0, 2, 1, 3)], 4, 'sans croisement')
inspect(initial, guide, 4, 'vision', stale=True)
inspect(open_fixture('masque', rectangle), rectangle, 7, 'manquants')
inspect(open_fixture('reflet', rectangle), rectangle, 7, 'ambigus')
inspect(open_fixture('prolongement', rectangle), rectangle, 4, 'se prolonge')
for name, points in [('concave_l', concave_l), ('concave_u', concave_u)]:
    baseline = open_fixture(name, points)
    for ordered in (points, list(reversed(points))):
        result = inspect(baseline, [(x+0.75,y) for x,y in ordered], 4)
        offsets = [math.hypot(c['intersection_px']['x']-x, c['intersection_px']['y']-y)
                   for c,(x,y) in zip(result['corners'], ordered)]
        assert len(result['corners']) == len(result['sides']) == len(ordered)
        assert max(offsets) < 0.2, offsets
        assert all(s['evidence']['coverage_fraction'] == 1 for s in result['sides'])
inspect(open_fixture('concave_masque', concave_l), concave_l, 7, 'manquants')
inspect(open_fixture('concave_reflet', concave_l), concave_l, 7, 'ambigus')
print('12 cas MCP verifies. Erreur maximale du trapeze en px:', max(errors))
print('Journal:', client.log_path)
'@ | python -
```

Le trapèze est restitué avec quatre côtés et quatre intersections à moins
de 0,2 px de la référence synthétique, malgré un guide décalé de 0,75 px.
Le guide croisé, la révision périmée, le masquage, le reflet et le
prolongement donnent chacun un refus. Après chaque diagnostic, les états
de mesure et d’inspection doivent correspondre à ceux lus juste avant.

## Résultats exécutés pour les contours concaves

- 31 tests Rust de vision réussis avec `cargo test --locked dper_vision::`,
  dont quatre tests polygonaux. Ils incluent les formes en L et en U dans
  les deux sens, les guides en étoile, les contacts et les sommets répétés.
- Douze cas réussis par HTTP MCP sur un hôte Windows dédié sans fenêtre,
  au port 18784 pour le rejeu exécuté. Écart maximal de 0,0044 px sur le
  trapèze synthétique et inférieur à 0,2 px sur les quatre cas concaves.
  Les états de mesure et d’inspection sont comparés après chaque diagnostic.
  Arrêt normal de l’hôte.
- Compilation `cargo build --locked` réussie, avec 39 avertissements
  de code inutilisé existants.
- Journal local `O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE\output\dpm12-polygon-mcp\rpc_evidence_002.json`.
- Journaux de l’hôte `O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE\output\dpm12-concave-host-20261115-211224-033`.

Ce diagnostic MCP traite un contour polygonal unique. DPM-12 reste ouvert
pour les assemblages et les autres familles de formes. Les valeurs de cette
recette proviennent d’images synthétiques.
