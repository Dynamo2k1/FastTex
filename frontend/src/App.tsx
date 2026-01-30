import { useState, useCallback } from 'react'
import { Panel, PanelGroup, PanelResizeHandle } from 'react-resizable-panels'
import { 
  FileText, 
  FolderOpen, 
  FolderClosed, 
  Play, 
  Loader2, 
  AlertCircle,
  CheckCircle,
  FileCode
} from 'lucide-react'
import Editor from './components/Editor'
import './App.css'

interface FileItem {
  name: string
  type: 'file' | 'folder'
  children?: FileItem[]
  isOpen?: boolean
}

function App() {
  const [projectId] = useState('demo-project')
  const [filePath, setFilePath] = useState('main.tex')
  const [compileStatus, setCompileStatus] = useState<'idle' | 'compiling' | 'success' | 'error'>('idle')
  const [compileMessage, setCompileMessage] = useState<string | null>(null)
  const [pdfUrl, setPdfUrl] = useState<string | null>(null)
  const apiUrl = import.meta.env.VITE_API_URL || 'http://localhost:8080'

  const [files, setFiles] = useState<FileItem[]>([
    { name: 'main.tex', type: 'file' },
    { 
      name: 'chapters', 
      type: 'folder', 
      isOpen: false,
      children: [
        { name: 'introduction.tex', type: 'file' },
        { name: 'methodology.tex', type: 'file' },
        { name: 'results.tex', type: 'file' },
      ]
    },
    { 
      name: 'figures', 
      type: 'folder', 
      isOpen: false,
      children: [
        { name: 'diagram.tex', type: 'file' },
      ]
    },
  ])

  const handleCompile = useCallback(async () => {
    setCompileStatus('compiling')
    setCompileMessage('Compiling document...')

    try {
      const response = await fetch(`${apiUrl}/api/compile`, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
        },
        body: JSON.stringify({
          projectId,
          filePath,
        }),
      })

      if (!response.ok) {
        throw new Error(`Compilation failed: ${response.statusText}`)
      }

      const blob = await response.blob()
      
      // Revoke previous URL to prevent memory leaks
      if (pdfUrl) {
        URL.revokeObjectURL(pdfUrl)
      }

      const newPdfUrl = URL.createObjectURL(blob)
      setPdfUrl(newPdfUrl)
      setCompileStatus('success')
      setCompileMessage('Compiled successfully!')
      
      // Clear success message after 3 seconds
      setTimeout(() => {
        if (compileStatus === 'success') {
          setCompileMessage(null)
        }
      }, 3000)
    } catch (error) {
      setCompileStatus('error')
      setCompileMessage(error instanceof Error ? error.message : 'Compilation failed')
    }
  }, [apiUrl, projectId, filePath, pdfUrl, compileStatus])

  const toggleFolder = (folderName: string) => {
    setFiles(prevFiles => 
      prevFiles.map(file => 
        file.name === folderName && file.type === 'folder'
          ? { ...file, isOpen: !file.isOpen }
          : file
      )
    )
  }

  const renderFileTree = (items: FileItem[], depth = 0) => {
    return items.map(item => (
      <div key={item.name}>
        <div 
          className={`file-item ${item.type === 'file' && item.name === filePath ? 'active' : ''}`}
          style={{ paddingLeft: `${12 + depth * 16}px` }}
          onClick={() => {
            if (item.type === 'folder') {
              toggleFolder(item.name)
            } else {
              setFilePath(item.name)
            }
          }}
        >
          <span className="file-icon">
            {item.type === 'folder' ? (
              item.isOpen ? <FolderOpen size={16} /> : <FolderClosed size={16} />
            ) : (
              <FileText size={16} />
            )}
          </span>
          <span className="file-name">{item.name}</span>
        </div>
        {item.type === 'folder' && item.isOpen && item.children && (
          <div className="folder-children">
            {renderFileTree(item.children, depth + 1)}
          </div>
        )}
      </div>
    ))
  }

  const getStatusIcon = () => {
    switch (compileStatus) {
      case 'compiling':
        return <Loader2 size={16} className="spin" />
      case 'success':
        return <CheckCircle size={16} />
      case 'error':
        return <AlertCircle size={16} />
      default:
        return null
    }
  }

  return (
    <div className="app">
      <header className="app-header">
        <div className="header-left">
          <div className="logo">
            <FileCode size={24} className="logo-icon" />
            <h1>Fast<span className="tex">T<sub>E</sub>X</span></h1>
          </div>
        </div>
        <div className="header-center">
          <span className="project-name">{projectId}</span>
        </div>
        <div className="header-right">
          <button 
            className={`compile-btn ${compileStatus === 'compiling' ? 'compiling' : ''}`} 
            onClick={handleCompile}
            disabled={compileStatus === 'compiling'}
          >
            {compileStatus === 'compiling' ? (
              <Loader2 size={16} className="spin" />
            ) : (
              <Play size={16} />
            )}
            <span>Compile</span>
          </button>
          {compileMessage && (
            <div className={`compile-status ${compileStatus}`}>
              {getStatusIcon()}
              <span>{compileMessage}</span>
            </div>
          )}
        </div>
      </header>
      
      <main className="app-main">
        <PanelGroup direction="horizontal" className="panel-group">
          {/* File Tree Sidebar */}
          <Panel defaultSize={15} minSize={10} maxSize={30} className="sidebar-panel">
            <aside className="file-tree">
              <div className="file-tree-header">
                <h3>Project Files</h3>
              </div>
              <div className="file-list">
                {renderFileTree(files)}
              </div>
            </aside>
          </Panel>

          <PanelResizeHandle className="resize-handle" />

          {/* Editor Panel */}
          <Panel defaultSize={45} minSize={25} className="editor-panel">
            <section className="editor-section">
              <div className="panel-header">
                <FileText size={14} />
                <span>{filePath}</span>
              </div>
              <Editor 
                projectId={projectId} 
                filePath={filePath}
                onCompile={handleCompile}
              />
            </section>
          </Panel>

          <PanelResizeHandle className="resize-handle" />

          {/* PDF Preview Panel */}
          <Panel defaultSize={40} minSize={20} className="preview-panel">
            <aside className="preview-section">
              <div className="panel-header">
                <FileText size={14} />
                <span>PDF Preview</span>
              </div>
              <div className="preview-content">
                {pdfUrl ? (
                  <iframe 
                    src={pdfUrl} 
                    className="pdf-viewer"
                    title="PDF Preview"
                  />
                ) : (
                  <div className="preview-placeholder">
                    <FileText size={48} className="placeholder-icon" />
                    <p>PDF preview will appear here after compilation</p>
                    <p className="hint">Press Ctrl+S or click Compile to build</p>
                  </div>
                )}
              </div>
            </aside>
          </Panel>
        </PanelGroup>
      </main>
      
      <footer className="app-footer">
        <span>FastTeX v0.1.0</span>
        <span className="separator">|</span>
        <span>Real-time collaborative LaTeX editing</span>
        <span className="separator">|</span>
        <span className="endpoint">API: {apiUrl}</span>
      </footer>
    </div>
  )
}

export default App
