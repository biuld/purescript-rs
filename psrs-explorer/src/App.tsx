import { Link, Navigate, NavLink, Route, Routes } from 'react-router';
import { IrDetailPage } from './pages/IrDetailPage';
import { OverviewPage } from './pages/OverviewPage';
import { PassDetailPage } from './pages/PassDetailPage';
import { TracePage } from './pages/TracePage';

export default function App() {
  return (
    <div className="app-shell">
      <header className="site-header">
        <Link to="/" className="wordmark">psrs <span>Explorer</span></Link>
        <nav aria-label="Primary navigation">
          <NavLink to="/" end>Architecture</NavLink>
          <NavLink to="/passes">Passes</NavLink>
          <NavLink to="/irs">IRs</NavLink>
          <NavLink to="/trace">Trace</NavLink>
        </nav>
      </header>
      <Routes>
        <Route path="/" element={<OverviewPage />} />
        <Route path="/passes" element={<PassDetailPage />} />
        <Route path="/passes/:passId" element={<PassDetailPage />} />
        <Route path="/irs" element={<IrDetailPage />} />
        <Route path="/irs/:representationId" element={<IrDetailPage />} />
        <Route path="/trace" element={<Navigate replace to="/trace/basic" />} />
        <Route path="/trace/:exampleId" element={<TracePage />} />
        <Route path="*" element={<OverviewPage />} />
      </Routes>
    </div>
  );
}
