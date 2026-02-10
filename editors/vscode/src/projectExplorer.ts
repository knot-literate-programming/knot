import * as vscode from 'vscode';
import * as fs from 'fs';
import * as path from 'path';
import { findProjectRoot, parseMainFromToml } from './utils';

export class KnotProjectProvider implements vscode.TreeDataProvider<ProjectItem> {
    private _onDidChangeTreeData: vscode.EventEmitter<ProjectItem | undefined | null | void> = new vscode.EventEmitter<ProjectItem | undefined | null | void>();
    readonly onDidChangeTreeData: vscode.Event<ProjectItem | undefined | null | void> = this._onDidChangeTreeData.event;

    constructor() {
        vscode.window.onDidChangeActiveTextEditor(() => this.refresh());
    }

    refresh(): void {
        this._onDidChangeTreeData.fire();
    }

    getTreeItem(element: ProjectItem): vscode.TreeItem {
        return element;
    }

    async getChildren(element?: ProjectItem): Promise<ProjectItem[]> {
        if (element) {
            return this.getDirectoryItems(element.resourceUri!.fsPath, false);
        }

        let root: string | null = null;
        const editor = vscode.window.activeTextEditor;
        
        if (editor) {
            root = findProjectRoot(path.dirname(editor.document.uri.fsPath));
        }

        if (!root) {
            const knotTomls = await vscode.workspace.findFiles('**/knot.toml', '**/node_modules/**', 1);
            if (knotTomls.length > 0) {
                root = path.dirname(knotTomls[0].fsPath);
            }
        }

        if (!root) return [];

        return this.getDirectoryItems(root, true);
    }

    private getDirectoryItems(dirPath: string, isProjectRoot: boolean): ProjectItem[] {
        const items: ProjectItem[] = [];
        if (!fs.existsSync(dirPath)) return [];
        
        const dirents = fs.readdirSync(dirPath, { withFileTypes: true });
        
        let mainFileName = 'main.knot';
        if (isProjectRoot) {
            const knotTomlPath = path.join(dirPath, 'knot.toml');
            if (fs.existsSync(knotTomlPath)) {
                mainFileName = parseMainFromToml(knotTomlPath);
                
                // Add knot.toml at the very top
                const configItem = new ProjectItem(
                    'knot.toml',
                    vscode.TreeItemCollapsibleState.None,
                    vscode.Uri.file(knotTomlPath),
                    'config'
                );
                configItem.description = '[Project Config]';
                configItem.iconPath = new vscode.ThemeIcon('settings-gear');
                configItem.command = { command: 'vscode.open', title: "Open File", arguments: [vscode.Uri.file(knotTomlPath)] };
                items.push(configItem);
            }
        }

        const knotGroups: ProjectItem[] = [];
        const knotFiles: ProjectItem[] = [];
        const folders: ProjectItem[] = [];
        const files: ProjectItem[] = [];

        const ignored = ['.git', '.knot_cache', '_knot_files', 'node_modules', '.vscode', 'target', '.DS_Store'];

        for (const dirent of dirents) {
            if (ignored.includes(dirent.name) || dirent.name.endsWith('.pdf') || (isProjectRoot && dirent.name === 'knot.toml')) {
                continue;
            }

            const fullPath = path.join(dirPath, dirent.name);

            if (dirent.isDirectory()) {
                if (this.containsKnotFiles(fullPath)) {
                    const groupItem = new ProjectItem(
                        isProjectRoot ? `[${dirent.name}]` : dirent.name,
                        vscode.TreeItemCollapsibleState.Collapsed,
                        vscode.Uri.file(fullPath),
                        'chapter-group'
                    );
                    groupItem.iconPath = new vscode.ThemeIcon('library');
                    knotGroups.push(groupItem);
                } else {
                    const folderItem = new ProjectItem(
                        dirent.name,
                        vscode.TreeItemCollapsibleState.Collapsed,
                        vscode.Uri.file(fullPath),
                        'folder'
                    );
                    folders.push(folderItem);
                }
            } else {
                const isMain = isProjectRoot && dirent.name === mainFileName;
                const isKnot = dirent.name.endsWith('.knot');

                const item = new ProjectItem(
                    dirent.name,
                    vscode.TreeItemCollapsibleState.None,
                    vscode.Uri.file(fullPath),
                    isMain ? 'main' : (isKnot ? 'knot-file' : 'file')
                );
                
                item.command = { command: 'vscode.open', title: "Open File", arguments: [vscode.Uri.file(fullPath)] };

                if (isMain) {
                    item.description = '[Entry Point]';
                    item.iconPath = new vscode.ThemeIcon('star-full');
                    items.push(item);
                } else if (isKnot) {
                    knotFiles.push(item);
                } else {
                    files.push(item);
                }
            }
        }

        knotGroups.sort((a, b) => a.label!.toString().localeCompare(b.label!.toString()));
        knotFiles.sort((a, b) => a.label!.toString().localeCompare(b.label!.toString()));
        folders.sort((a, b) => a.label!.toString().localeCompare(b.label!.toString()));
        files.sort((a, b) => a.label!.toString().localeCompare(b.label!.toString()));

        return [...items, ...knotGroups, ...knotFiles, ...folders, ...files];
    }

    private containsKnotFiles(dirPath: string): boolean {
        try {
            const contents = fs.readdirSync(dirPath);
            return contents.some(f => f.endsWith('.knot'));
        } catch (e) {
            return false;
        }
    }
}

class ProjectItem extends vscode.TreeItem {
    constructor(
        public readonly label: string,
        public readonly collapsibleState: vscode.TreeItemCollapsibleState,
        public readonly resourceUri: vscode.Uri,
        public readonly contextValue: string
    ) {
        super(label, collapsibleState);
        this.resourceUri = resourceUri;
    }
}
