import React, { useEffect, useRef, useState } from 'react';
import { EditorState } from '@codemirror/state';
import { EditorView, keymap, lineNumbers, highlightActiveLine } from '@codemirror/view';
import { defaultKeymap, history, historyKeymap } from '@codemirror/commands';
import { syntaxHighlighting, defaultHighlightStyle, bracketMatching } from '@codemirror/language';
import { searchKeymap } from '@codemirror/search';
import { autocompletion, completionKeymap } from '@codemirror/autocomplete';
import * as Y from 'yjs';
import { yCollab, yUndoManagerKeymap } from 'y-codemirror.next';
import { WebsocketProvider } from 'y-websocket';
import { IndexeddbPersistence } from 'y-indexeddb';

interface EditorProps {
  projectId: string;
  filePath: string;
  onCompile?: () => void;
}

/**
 * Collaborative LaTeX editor using CodeMirror 6 and Yjs
 * 
 * Features:
 * - Real-time collaboration via Yjs CRDT
 * - Offline support via IndexedDB persistence
 * - LaTeX syntax highlighting
 * - Presence awareness (cursors, selections)
 */
export function Editor({ projectId, filePath, onCompile }: EditorProps) {
  const editorRef = useRef<HTMLDivElement>(null);
  const viewRef = useRef<EditorView | null>(null);
  const [connected, setConnected] = useState(false);
  const [users, setUsers] = useState<number>(0);

  useEffect(() => {
    if (!editorRef.current) return;

    // Create Yjs document
    const ydoc = new Y.Doc();
    
    // Get the Y.Text for this file
    const ytext = ydoc.getText(`file:${filePath}`);

    // Set up WebSocket provider for sync
    const wsProvider = new WebsocketProvider(
      `ws://${window.location.host}/sync/${projectId}`,
      projectId,
      ydoc
    );

    wsProvider.on('status', (event: { status: string }) => {
      setConnected(event.status === 'connected');
    });

    wsProvider.awareness.on('change', () => {
      setUsers(wsProvider.awareness.getStates().size);
    });

    // Set up IndexedDB persistence for offline support
    const indexeddbProvider = new IndexeddbPersistence(
      `fasttex-${projectId}`,
      ydoc
    );

    // Create user color for presence
    const userColor = generateUserColor();
    wsProvider.awareness.setLocalStateField('user', {
      name: 'Anonymous',
      color: userColor.color,
      colorLight: userColor.light,
    });

    // Set up CodeMirror with Yjs
    const state = EditorState.create({
      doc: ytext.toString(),
      extensions: [
        lineNumbers(),
        highlightActiveLine(),
        bracketMatching(),
        history(),
        syntaxHighlighting(defaultHighlightStyle),
        autocompletion(),
        keymap.of([
          ...defaultKeymap,
          ...historyKeymap,
          ...searchKeymap,
          ...completionKeymap,
          ...yUndoManagerKeymap,
          // Compile on Cmd/Ctrl+S
          {
            key: 'Mod-s',
            run: () => {
              onCompile?.();
              return true;
            },
          },
        ]),
        // Yjs collaboration extension
        yCollab(ytext, wsProvider.awareness),
        // LaTeX mode (using markdown as placeholder)
        // In production, use a proper LaTeX grammar
      ],
    });

    const view = new EditorView({
      state,
      parent: editorRef.current,
    });

    viewRef.current = view;

    return () => {
      view.destroy();
      wsProvider.disconnect();
      indexeddbProvider.destroy();
      ydoc.destroy();
    };
  }, [projectId, filePath, onCompile]);

  return (
    <div className="editor-container">
      <div className="editor-status">
        <span className={`connection-status ${connected ? 'connected' : 'disconnected'}`}>
          {connected ? '●' : '○'} {connected ? 'Connected' : 'Offline'}
        </span>
        <span className="user-count">{users} user{users !== 1 ? 's' : ''}</span>
      </div>
      <div ref={editorRef} className="editor" />
    </div>
  );
}

/**
 * Generate a random user color for presence
 */
function generateUserColor() {
  const colors = [
    { color: '#30bced', light: '#30bced33' },
    { color: '#6eeb83', light: '#6eeb8333' },
    { color: '#ffbc42', light: '#ffbc4233' },
    { color: '#ecd444', light: '#ecd44433' },
    { color: '#ee6352', light: '#ee635233' },
    { color: '#9ac2c9', light: '#9ac2c933' },
    { color: '#8acb88', light: '#8acb8833' },
    { color: '#1be7ff', light: '#1be7ff33' },
  ];
  return colors[Math.floor(Math.random() * colors.length)];
}

export default Editor;
