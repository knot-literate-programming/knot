/** Versioned server events. A generation is local to one canonical project root. */
export interface CompilationEvent {
    uri: string;
    project: string;
    generation: number;
    versions: Record<string, number>;
}
export type CompilationState = 'running' | 'modified' | 'success' | 'failed';
interface Entry extends CompilationEvent { state: CompilationState }

/** Keeps late results from clearing newer edits, including edits in an include. */
export class CompilationStatus {
    private readonly projects = new Map<string, Entry>();

    begin(event: CompilationEvent, running: boolean, versions: ReadonlyMap<string, number>): void {
        const previous = this.projects.get(event.project);
        if (previous && previous.generation >= event.generation) return;
        this.projects.set(event.project, {
            ...event,
            state: running && this.matches(event, versions) ? 'running' : 'modified',
        });
    }

    complete(event: CompilationEvent & { success: boolean }, versions: ReadonlyMap<string, number>): void {
        const current = this.projects.get(event.project);
        if (!current || current.generation !== event.generation || current.state !== 'running') return;
        current.state = this.matches(current, versions) ? (event.success ? 'success' : 'failed') : 'modified';
    }

    edit(uri: string): void {
        for (const entry of this.projects.values()) {
            if (uri in entry.versions) entry.state = 'modified';
        }
    }

    forDocument(uri: string, version: number): CompilationState {
        for (const entry of this.projects.values()) {
            if (uri in entry.versions) {
                return entry.versions[uri] === version ? entry.state : 'modified';
            }
        }
        return 'modified';
    }

    private matches(event: CompilationEvent, versions: ReadonlyMap<string, number>): boolean {
        return Object.entries(event.versions).every(([uri, version]) => {
            const current = versions.get(uri);
            return current !== undefined && current === version;
        });
    }
}
