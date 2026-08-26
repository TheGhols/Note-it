import { HostToWebviewMessage, WebviewToHostMessage } from './types.ts';

export class NativeBridge {
  private messageListeners: Array<(message: HostToWebviewMessage) => void> = [];

  constructor() {
    window.handleHostMessage = (rawMessage: string | HostToWebviewMessage) => {
      try {
        const message: HostToWebviewMessage =
          typeof rawMessage === 'string' ? JSON.parse(rawMessage) : rawMessage;
        for (const listener of this.messageListeners) {
          listener(message);
        }
      } catch (err) {
        console.error('Failed to parse host message:', err, rawMessage);
      }
    };
  }

  public sendMessage(message: WebviewToHostMessage): void {
    if (window.webkit?.messageHandlers?.noteItHost) {
      window.webkit.messageHandlers.noteItHost.postMessage(JSON.stringify(message));
    } else {
      // In standalone browser/development mode, log to console
      console.log('[NativeBridge -> Host]', message);
    }
  }

  public onMessage(listener: (message: HostToWebviewMessage) => void): () => void {
    this.messageListeners.push(listener);
    return () => {
      this.messageListeners = this.messageListeners.filter((l) => l !== listener);
    };
  }
}

export const bridge = new NativeBridge();
