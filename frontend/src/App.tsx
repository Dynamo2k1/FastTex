import { useState } from 'react'
import Editor from './components/Editor'
import './App.css'

function App() {
  const [projectId] = useState('demo-project')
  const [filePath] = useState('main.tex')
  const [compileStatus, setCompileStatus] = useState<string | null>(null)
  const apiUrl = import.meta.env.VITE_API_URL || 'http://localhost:8080'

  const handleCompile = () => {
    setCompileStatus('Compiling...')
    // In production, this would trigger actual compilation via WebSocket
    setTimeout(() => {
      setCompileStatus('Compiled successfully!')
      setTimeout(() => setCompileStatus(null), 3000)
    }, 1500)
  }

  return (
    <div className="app">
      <header className="app-header">
          <div className="logo">
            <h1>Fast<span className="tex">T<sub>E</sub>X</span></h1>
          </div>
          <div className="endpoint">
            Backend: <code>{apiUrl}</code>
          </div>
        <nav className="nav">
          <button className="compile-btn" onClick={handleCompile}>
            Compile (Ctrl+S)
          </button>
          {compileStatus && <span className="compile-status">{compileStatus}</span>}
        </nav>
      </header>
      
      <main className="app-main">
        <aside className="file-tree">
          <div className="file-tree-header">
            <h3>Project Files</h3>
          </div>
          <ul className="file-list">
            <li className="file-item active">
              <span className="file-icon">📄</span>
              main.tex
            </li>
            <li className="file-item">
              <span className="file-icon">📁</span>
              chapters/
            </li>
            <li className="file-item">
              <span className="file-icon">📁</span>
              figures/
            </li>
          </ul>
        </aside>
        
        <section className="editor-panel">
          <Editor 
            projectId={projectId} 
            filePath={filePath}
            onCompile={handleCompile}
          />
        </section>
        
        <aside className="preview-panel">
          <div className="preview-header">
            <h3>PDF Preview</h3>
          </div>
          <div className="preview-content">
            <div className="preview-placeholder">
              <p>PDF preview will appear here after compilation</p>
              <p className="hint">Press Ctrl+S or click Compile to build</p>
            </div>
          </div>
        </aside>
      </main>
      
      <footer className="app-footer">
        <span>FastTeX v0.1.0</span>
        <span className="separator">|</span>
        <span>Real-time collaborative LaTeX editing</span>
      </footer>
    </div>
  )
}

export default App
