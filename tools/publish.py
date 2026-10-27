#!/usr/bin/env python3
"""Validate and publish a source-only snapshot from the read-only-Git workspace.
Game inputs, generated artifacts, credentials and READMEs are never copied.
Runs only when explicitly invoked; it installs no background watcher.
"""
import argparse
import json
from pathlib import Path
import shutil
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
DIRECTORIES = ('crates', 'native', 'third_party', 'tools', 'docs')
ROOT_FILES = ('.gitignore', 'Cargo.toml', 'Cargo.lock', 'LICENSE', 'NOTICE')


def run(*args, **kwargs):
    return subprocess.run(args, check=True, **kwargs)


def source_files():
    paths = [ROOT / name for name in ROOT_FILES if (ROOT / name).is_file()]
    for name in DIRECTORIES:
        paths.extend(path for path in (ROOT / name).rglob('*')
                     if path.is_file() and '__pycache__' not in path.parts
                     and not path.name.lower().startswith('readme')
                     and path.suffix != '.pyc')
    for path in sorted(paths):
        if path.is_symlink():
            raise SystemExit(f'Refusing source symlink: {path.relative_to(ROOT)}')
        yield path


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--repository', required=True, help='Authorized GitHub HTTPS repository URL')
    parser.add_argument('--message', required=True)
    args = parser.parse_args()
    if not args.repository.startswith('https://github.com/') or not args.repository.endswith('.git'):
        parser.error('Use a GitHub HTTPS .git repository URL without credentials')
    owner_name = args.repository.removeprefix('https://github.com/').removesuffix('.git')
    if len(owner_name.split('/')) != 2 or any(c not in 'abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-_./' for c in owner_name):
        parser.error('Invalid GitHub owner/repository')
    # Gate every commit/push on the workspace's required tests and supplied inputs.
    run(sys.executable, str(ROOT / 'tools/validate.py'), '--game', str(ROOT / 'base'), cwd=ROOT)
    account = json.loads(subprocess.check_output(['gh', 'api', 'user'], text=True))
    remote = json.loads(subprocess.check_output(['gh', 'repo', 'view', owner_name,
                                               '--json', 'isPrivate'], text=True))
    if not remote['isPrivate']:
        raise SystemExit('Refusing publication to a non-private repository')
    checkout = ROOT / 'private/publish'
    checkout.mkdir(parents=True, exist_ok=True)
    if not (checkout / '.git').exists():
        run('git', 'init', '-b', 'main', str(checkout))
        run('git', '-C', str(checkout), 'remote', 'add', 'origin', args.repository)
    configured = subprocess.check_output(['git', '-C', str(checkout), 'remote', 'get-url', 'origin'], text=True).strip()
    if configured != args.repository:
        raise SystemExit('Existing publication checkout has a different remote')
    git = ['git', '-C', str(checkout)]
    run(*git, 'config', 'user.name', account['login'])
    run(*git, 'config', 'user.email', f"{account['id']}+{account['login']}@users.noreply.github.com")
    run(*git, 'config', 'credential.https://github.com.helper', '!gh auth git-credential')
    files = {path.relative_to(ROOT): path for path in source_files()}
    # Delete only files this publisher recorded in its previous source snapshot.
    manifest_path = ROOT / 'private/publish-files.json'
    previous = json.loads(manifest_path.read_text()) if manifest_path.exists() else []
    for name in previous:
        relative = Path(name)
        if relative.is_absolute() or '..' in relative.parts or relative.parts[0] == '.git':
            raise SystemExit('Invalid prior publication path')
        if relative not in files:
            (checkout / relative).unlink(missing_ok=True)
    for relative, source in files.items():
        destination = checkout / relative
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(source, destination)
    manifest_path.write_text(json.dumps([str(p) for p in files], indent=2) + '\n')
    run(*git, 'add', '-A', '--', *sorted({str(p) for p in files} | set(previous)))
    staged = subprocess.run([*git, 'diff', '--cached', '--quiet'], check=False)
    if staged.returncode == 1:
        run(*git, 'commit', '-m', args.message)
    elif staged.returncode:
        raise SystemExit(staged.returncode)
    run(*git, 'push', '-u', 'origin', 'main')


if __name__ == '__main__':
    main()
