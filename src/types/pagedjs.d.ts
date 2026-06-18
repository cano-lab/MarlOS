declare module "pagedjs" {
  export class Previewer {
    constructor(options?: any);
    preview(
      content: string | Element,
      stylesheets: Array<string>,
      renderTo: Element,
    ): Promise<{ total: number; pages: any[] }>;
  }
  export class Chunker {
    constructor(options?: any);
  }
  export class Polisher {
    constructor(options?: any);
  }
  export class Handler {
    constructor(chunker?: any, polisher?: any, caller?: any);
  }
  export const registeredHandlers: any[];
  export function registerHandlers(...handlers: any[]): void;
  export function initializeHandlers(...args: any[]): void;
}
