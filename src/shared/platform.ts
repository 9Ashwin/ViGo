/** Desktop platform identifiers. Shared code takes this instead of reading process.platform. */
export type Platform = 'win' | 'mac' | 'linux';

export const platformOf = (nodePlatform: string): Platform => (nodePlatform === 'darwin' ? 'mac' : nodePlatform === 'linux' ? 'linux' : 'win');
