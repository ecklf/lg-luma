#!/usr/bin/python2
"""Reversible webOS 4 Home routing; Python 2.7, no third-party dependencies."""
from __future__ import print_function
import json
import os
import select
import shutil
import stat
import subprocess
import sys
import time

STATE = '/var/lib/luma-legacy-home'
FILTER = STATE + '/legacy-home-filter.js'
HOOK = '/var/lib/webosbrew/init.d/64-luma-legacy-home'
KEY = 'com.webos.surfacemanager.keyFilters'
APP = 'com.ecklf.luma'


def luna(service, method, payload, timeout=8):
    process = subprocess.Popen(
        ['luna-send', '-n', '1', '-w', str(timeout * 1000),
         'luna://' + service + '/' + method, json.dumps(payload)],
        stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    try:
        if not select.select([process.stdout], [], [], timeout + 1)[0]:
            raise RuntimeError('No response: ' + method)
        reply = json.loads(process.stdout.readline())
        if reply.get('returnValue') is not True:
            raise RuntimeError(method + ': ' + json.dumps(reply))
        return reply
    finally:
        if process.poll() is None:
            process.terminate()
        process.wait()


def filters():
    return luna('com.webos.service.config', 'getConfigs',
                {'configNames': [KEY]})['configs'][KEY]


def configure(enabled):
    current = filters()
    updated = [entry for entry in current if entry.get('file') != FILTER]
    if enabled:
        if not any(entry.get('handler') == 'handleSystemKeys' for entry in updated):
            raise RuntimeError('No compatible LG system key handler found')
        # Retain LG's screen-saver, accessibility and other pre-Home policies.
        position = next((i for i, entry in enumerate(updated)
                         if entry.get('handler') == 'handleSystemKeys'), 0)
        updated.insert(position, {'file': FILTER, 'handler': 'lumaLegacyHomeKey'})
    if current != updated:
        luna('com.webos.service.config', 'setConfigs', {'configs': {KEY: updated}})


def open_home():
    luna('com.webos.applicationManager', 'launch', {'id': APP})


def prepare_state():
    if not os.path.lexists(STATE):
        os.mkdir(STATE, 0o755)
    info = os.lstat(STATE)
    if not stat.S_ISDIR(info.st_mode) or info.st_uid != 0:
        raise RuntimeError('Home state must be a root-owned directory, not a symlink')
    for name in ('original-keyfilters.json', 'legacy-home.py', 'legacy-home-filter.js'):
        target = STATE + '/' + name
        if os.path.lexists(target):
            entry = os.lstat(target)
            if not stat.S_ISREG(entry.st_mode) or entry.st_uid != 0:
                raise RuntimeError('Refusing non-regular or foreign Home state: ' + name)


def main():
    command = sys.argv[1] if len(sys.argv) > 1 else 'status'
    if command not in ('enable', 'disable', 'status', 'boot'):
        raise RuntimeError('Usage: legacy-home.py enable|disable|status|boot')
    if os.geteuid() != 0:
        raise RuntimeError('Root required')
    if command == 'status':
        print(json.dumps({'enabled': any(entry.get('file') == FILTER for entry in filters()),
                          'bootHook': os.path.isfile(HOOK), 'homeApp': APP}))
    elif command == 'enable':
        if 'release 4.' not in open('/etc/starfish-release').read():
            raise RuntimeError('This integration is only for webOS 4')
        luna('com.webos.applicationManager', 'getAppInfo', {'id': APP})
        prepare_state()
        source = os.path.dirname(os.path.realpath(__file__))
        backup = STATE + '/original-keyfilters.json'
        if not os.path.exists(backup):
            with open(backup, 'w') as stream:
                json.dump(filters(), stream, indent=2)
        for name in ('legacy-home-filter.js', 'legacy-home.py'):
            target = STATE + '/' + name
            origin = source + '/' + name
            if os.path.realpath(origin) != os.path.realpath(target):
                shutil.copyfile(origin, target)
            os.chmod(target, 0o755 if name.endswith('.py') else 0o644)
        configure(True)
        try:
            with open(HOOK, 'w') as stream:
                stream.write('#!/bin/sh\nexec /usr/bin/python2 ' + STATE + '/legacy-home.py boot\n')
            os.chmod(HOOK, 0o755)
            open_home()
        except Exception:
            configure(False)
            if os.path.exists(HOOK):
                os.unlink(HOOK)
            raise
        print('Enabled Luma Home routing for webOS 4')
    elif command == 'disable':
        configure(False)
        if os.path.exists(HOOK):
            os.unlink(HOOK)
        print('Restored LG Home routing; backups retained in ' + STATE)
    else:
        for attempt in range(12):
            try:
                configure(True)
                open_home()
                print('Luma Home started')
                return
            except Exception as error:
                if attempt == 11:
                    raise error
                time.sleep(2)


if __name__ == '__main__':
    try:
        main()
    except Exception as error:
        print(str(error), file=sys.stderr)
        sys.exit(1)
