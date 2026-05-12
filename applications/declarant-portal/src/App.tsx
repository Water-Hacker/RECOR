import { DeclarationForm } from './features/declaration/DeclarationForm';

const API_BASE_URL =
  (import.meta.env.VITE_DECLARATION_API_URL as string | undefined) ??
  'http://localhost:8080';

export function App() {
  return (
    <div className="min-h-screen">
      <header className="bg-recor-deep text-white">
        <div className="mx-auto max-w-4xl px-4 py-8">
          <h1 className="text-3xl font-semibold">RÉCOR</h1>
          <p className="mt-1 text-sm text-blue-100">
            Registre de l'Effective Contrôle et Origine Réelle ·
            National Beneficial Ownership Registry of Cameroon
          </p>
        </div>
      </header>

      <main className="mx-auto max-w-4xl px-4 py-8">
        <section className="space-y-2">
          <h2 className="text-2xl font-semibold text-slate-900">
            File a beneficial ownership declaration
          </h2>
          <p className="text-slate-700">
            Every legal entity operating in Cameroon must disclose the natural
            person who ultimately controls it. Your submission is signed
            cryptographically by your browser; the signing key is generated
            here and never leaves your device.
          </p>
        </section>

        <section className="mt-8 rounded-lg bg-white p-6 shadow-sm ring-1 ring-slate-200">
          <DeclarationForm apiBaseUrl={API_BASE_URL} />
        </section>

        <footer className="mt-12 text-center text-xs text-slate-500">
          <p>RÉCOR Declarant Portal · v0.1.0</p>
          <p className="mt-1">
            Submissions are encrypted in transit. The cryptographic receipt
            allows you to verify what you submitted years later.
          </p>
        </footer>
      </main>
    </div>
  );
}
