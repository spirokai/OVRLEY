"""Throwaway: lower an OpenFreeMap style to the subset maplibre-rs reads.

Usage: convert_style.py <in.json> <out.json> <eval_zoom>
"""
import json
import math
import re
import sys

src, dst, ZOOM = sys.argv[1], sys.argv[2], float(sys.argv[3])
dropped = {}


def note(kind):
    dropped[kind] = dropped.get(kind, 0) + 1


def is_get(x):
    return isinstance(x, list) and len(x) == 2 and x[0] == 'get'


def is_geom(x):
    return isinstance(x, list) and x and x[0] == 'geometry-type'


def as_values(values):
    return [str(v) for v in (values if isinstance(values, list) else [values])]


def lower_filter(f):
    """Returns a legacy filter, or None when the clause cannot be expressed."""
    if not isinstance(f, list) or not f:
        return None
    op = f[0]
    if op in ('all', 'any', 'none'):
        parts = [lower_filter(x) for x in f[1:]]
        parts = [p for p in parts if p is not None]
        return [op, *parts] if parts else None
    if op == '!' and len(f) == 2:
        inner = f[1]
        if isinstance(inner, list) and inner and inner[0] == 'has':
            return ['!has', inner[1]]
        lowered = lower_filter(inner)
        return ['none', lowered] if lowered else None
    if op == 'has':
        return ['has', f[1]]
    if op in ('==', '!=') and len(f) == 3:
        if is_geom(f[1]):
            return None
        if is_get(f[1]):
            return [op, f[1][1], f[2]]
        if isinstance(f[1], str):
            return f
    if op == 'match' and len(f) == 5:
        if is_geom(f[1]):
            return None
        if is_get(f[1]) and f[3] is True and f[4] is False:
            return ['in', f[1][1], *as_values(f[2])]
        if is_get(f[1]) and f[3] is False and f[4] is True:
            return ['!in', f[1][1], *as_values(f[2])]
    if op in ('in', '!in') and isinstance(f[1], str):
        return [op, f[1], *as_values(f[2:])]
    note(f'filter op {op}')
    return None


def at_zoom(v):
    """Evaluates a zoom interpolate/step, or returns the value unchanged."""
    if isinstance(v, list) and v and v[0] == 'interpolate' and v[2] == ['zoom']:
        stops = list(zip(v[3::2], v[4::2]))
        if ZOOM <= stops[0][0]:
            return stops[0][1]
        for (z0, a), (z1, b) in zip(stops, stops[1:]):
            if ZOOM <= z1:
                if isinstance(a, (int, float)) and isinstance(b, (int, float)):
                    return a + (b - a) * (ZOOM - z0) / (z1 - z0)
                return b if ZOOM - z0 > z1 - ZOOM else a
        return stops[-1][1]
    if isinstance(v, list) and v and v[0] == 'step' and v[1] == ['zoom']:
        result = v[2]
        for z, out in zip(v[3::2], v[4::2]):
            if ZOOM >= z:
                result = out
        return result
    return v


def with_alpha(color, alpha):
    if alpha >= 1 or not isinstance(color, str):
        return color
    c = color.strip()
    m = re.match(r'hsla?\(([^,]+),([^,]+),([^,)]+)(?:,([^)]+))?\)', c)
    if m:
        a = float(m.group(4)) if m.group(4) else 1.0
        return f'hsla({m.group(1)},{m.group(2)},{m.group(3)},{a * alpha:.3f})'
    m = re.match(r'rgba?\(([^,]+),([^,]+),([^,)]+)(?:,([^)]+))?\)', c)
    if m:
        a = float(m.group(4)) if m.group(4) else 1.0
        return f'rgba({m.group(1)},{m.group(2)},{m.group(3)},{a * alpha:.3f})'
    if re.match(r'#[0-9a-fA-F]{6}$', c):
        r, g, b = (int(c[i:i + 2], 16) for i in (1, 3, 5))
        return f'rgba({r},{g},{b},{alpha:.3f})'
    return color


style = json.load(open(src, encoding='utf8'))
out_layers = []
for layer in style['layers']:
    kind = layer['type']
    if layer.get('source') == 'ne2_shaded':
        continue
    if kind not in ('background', 'fill', 'line', 'symbol'):
        note(f'layer type {kind}')
        continue
    source_layer = layer.get('source-layer')
    paint = layer.get('paint', {})
    new = {'id': layer['id'], 'type': kind}
    if 'source' in layer:
        new['source'] = layer['source']
    if source_layer:
        new['source-layer'] = source_layer
    for key, fn in (('minzoom', math.ceil), ('maxzoom', math.ceil)):
        if key in layer:
            new[key] = int(fn(layer[key]))
    if 'filter' in layer:
        lowered = lower_filter(layer['filter'])
        if lowered:
            new['filter'] = lowered

    if kind == 'background':
        new['paint'] = {'background-color': at_zoom(paint.get('background-color', '#ffffff'))}
    elif kind == 'fill':
        if 'fill-pattern' in paint:
            note('fill-pattern layer')
            continue
        # Polygon fills from line-only source layers have no safe legacy geometry filter.
        if source_layer in ('transportation', 'aeroway'):
            note('fill on line source layer')
            continue
        color = at_zoom(paint.get('fill-color', '#000000'))
        opacity = at_zoom(paint.get('fill-opacity', 1))
        new['paint'] = {'fill-color': with_alpha(color, opacity if isinstance(opacity, (int, float)) else 1)}
    elif kind == 'line':
        color = at_zoom(paint.get('line-color', '#000000'))
        opacity = at_zoom(paint.get('line-opacity', 1))
        new['paint'] = {
            'line-color': with_alpha(color, opacity if isinstance(opacity, (int, float)) else 1),
            'line-width': at_zoom(paint.get('line-width', 1)),
        }
        if 'line-dasharray' in paint:
            note('line-dasharray (drawn solid)')
    else:
        layout = layer.get('layout', {})
        # Road and place names only, as the widget will use.
        if 'text-field' not in layout or source_layer not in ('transportation_name', 'place', 'water_name'):
            note('symbol layer outside road/place/water names')
            continue
        if 'ref' in json.dumps(layout['text-field']):
            note('shield layer')
            continue
        new['layout'] = {'text-field': '{name:latin}', 'text-size': at_zoom(layout.get('text-size', 14))}
    out_layers.append(new)

out = {'version': 8, 'name': style.get('name'), 'sources': {}, 'layers': out_layers}
json.dump(out, open(dst, 'w', encoding='utf8'))
print(f'{src.rsplit("/", 1)[-1]}: {len(style["layers"])} -> {len(out_layers)} layers; dropped: {dropped}')
