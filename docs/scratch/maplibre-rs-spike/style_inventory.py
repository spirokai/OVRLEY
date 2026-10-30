"""Throwaway: what the five OpenFreeMap styles use in the widget's z14-18 range."""
import collections
import json
import os
import sys

root = sys.argv[1]
READ_BY_ENGINE = {'background-color', 'fill-color', 'line-color', 'line-width', 'text-field', 'text-size'}
total_layers = collections.Counter()
props = collections.Counter()
props_by_style = collections.defaultdict(set)
value_kinds = collections.Counter()
filter_ops = collections.Counter()
expr_ops = collections.Counter()


def ops(v, into):
    if isinstance(v, list) and v:
        if isinstance(v[0], str):
            into[v[0]] += 1
        for x in v[1:] if isinstance(v[0], str) else v:
            ops(x, into)


for name in 'positron bright liberty dark fiord'.split():
    style = json.load(open(os.path.join(root, name + '.json'), encoding='utf8'))
    for layer in style['layers']:
        if layer.get('maxzoom', 99) <= 14 or layer.get('source') == 'ne2_shaded':
            continue
        total_layers[name] += 1
        total_layers[layer['type']] += 1
        if 'filter' in layer:
            ops(layer['filter'], filter_ops)
        for section in ('paint', 'layout'):
            for key, value in layer.get(section, {}).items():
                if key == 'visibility':
                    continue
                props[key] += 1
                props_by_style[key].add(name)
                if key in READ_BY_ENGINE:
                    head = value[0] if isinstance(value, list) and value else 'constant'
                    if isinstance(value, dict):
                        head = 'legacy-function'
                    value_kinds[(key, head)] += 1
                ops(value, expr_ops)

print('layers in z14-18:', dict(total_layers))
print('\nproperties (uses, styles); * = read by the engine today')
for key, count in props.most_common():
    mark = '*' if key in READ_BY_ENGINE else ' '
    print(f' {mark} {key:32} {count:4}  {len(props_by_style[key])}/5')
print('\nvalue forms of engine-read properties:')
for (key, head), count in sorted(value_kinds.items()):
    print(f'   {key:20} {head:14} {count}')
print('\nfilter operators:', dict(filter_ops.most_common()))
print('\nvalue expression operators:', dict(expr_ops.most_common()))
