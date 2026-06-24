import { useState, useEffect } from 'react';
import { Providers } from '@/components/providers';
import { SiteHeader } from '@/components/site-header';
import { Landing } from '@/pages/Landing';
import { Dashboard } from '@/pages/Dashboard';
import { Domain } from '@/pages/Domain';
import { Wallet } from '@/pages/Wallet';

function getRoute(): string {
  const hash = window.location.hash.replace(/^#/, '');
  return hash.split('?')[0] || '/';
}

function RouteView() {
  const [route, setRoute] = useState(getRoute());

  useEffect(() => {
    const onChange = () => setRoute(getRoute());
    window.addEventListener('hashchange', onChange);
    return () => window.removeEventListener('hashchange', onChange);
  }, []);

  useEffect(() => {
    if (!window.location.hash) {
      window.location.hash = '#/';
    }
  }, []);

  switch (route) {
    case '/dashboard':
      return <Dashboard />;
    case '/domain':
      return <Domain />;
    case '/wallet':
      return <Wallet />;
    default:
      return <Landing />;
  }
}

export function App() {
  return (
    <div className="antialiased min-h-screen">
      <Providers>
        <SiteHeader />
        <main className="mx-auto max-w-6xl px-4 py-8">
          <RouteView />
        </main>
      </Providers>
    </div>
  );
}
