#!/usr/bin/env python3
"""Extracts SELinux labels (security.selinux xattrs) of every file in the
ROM's APEX payloads into <rom>/.indroid-labels ("<guest path>\t<label>").
APEX contents are labeled inside their ext4 payload, not by file_contexts.
Needs: unzip, debugfs."""
import os, subprocess, sys, tempfile, zipfile

rom = sys.argv[1]
out = []
apexdir = os.path.join(rom, 'system/apex')
for f in sorted(os.listdir(apexdir)):
    if not (f.endswith('.apex') or f.endswith('.capex')):
        continue
    with tempfile.TemporaryDirectory() as t:
        z = zipfile.ZipFile(os.path.join(apexdir, f))
        if f.endswith('.capex'):
            z.extract('original_apex', t)
            z = zipfile.ZipFile(os.path.join(t, 'original_apex'))
        z.extract('apex_payload.img', t)
        z.extract('apex_manifest.pb', t)
        b = open(os.path.join(t, 'apex_manifest.pb'), 'rb').read()
        name = b[2:2 + b[1]].decode()
        img = os.path.join(t, 'apex_payload.img')
        root = os.path.join(rom, 'apex', name)
        paths = ['/']
        for d, dirs, files in os.walk(root):
            rel = os.path.relpath(d, root)
            for n in dirs + files:
                paths.append('/' + os.path.normpath(os.path.join(rel, n)).lstrip('./') if rel != '.' else '/' + n)
        cmds = ''.join('ea_get "%s" security.selinux\n' % p for p in paths)
        r = subprocess.run(['debugfs', '-f', '-', img], input=cmds, capture_output=True, text=True)
        cur = None
        labels = {}
        for line in (r.stdout + '\n').splitlines():
            if line.startswith('debugfs:') and 'ea_get' in line:
                cur = line.split('"')[1]
            elif cur and 'security.selinux' in line and '"' in line:
                labels[cur] = line.split('"')[1].replace('\\000', '')
                cur = None
        for p, l in labels.items():
            g = '/apex/%s%s' % (name, '' if p == '/' else p)
            out.append('%s\t%s' % (g, l))
        print(name, len(labels), file=sys.stderr)
open(os.path.join(rom, '.indroid-labels'), 'w').write('\n'.join(out) + '\n')
