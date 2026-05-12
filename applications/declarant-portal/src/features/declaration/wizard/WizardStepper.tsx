/**
 * Horizontal step indicator (1/4 Entity → 2/4 Owners → 3/4 Review → 4/4 Sign).
 *
 * Renders as an ordered list so assistive tech announces "step 2 of
 * 4, current". Each step is keyboard-non-interactive — the wizard
 * shell exposes Back/Forward buttons for navigation. The active
 * step carries `aria-current="step"` per the WAI-ARIA progress
 * pattern.
 */

import clsx from 'clsx';
import { useTranslation } from 'react-i18next';

import { LAST_STEP, WIZARD_STEPS, type WizardStep } from './types';

interface WizardStepperProps {
  current: WizardStep;
}

export function WizardStepper({ current }: WizardStepperProps) {
  const { t } = useTranslation();
  return (
    <nav
      aria-label={t('wizard.progressAria')}
      data-testid="wizard-stepper"
      className="mb-6"
    >
      <ol className="flex flex-wrap items-center gap-2 text-sm md:gap-4">
        {WIZARD_STEPS.map((step, idx) => {
          const isCurrent = step === current;
          const isComplete = step < current;
          return (
            <li key={step} className="flex items-center gap-2">
              <span
                aria-current={isCurrent ? 'step' : undefined}
                data-testid={`wizard-stepper-${step}`}
                className={clsx(
                  'inline-flex items-center gap-2 rounded-full px-3 py-1 font-medium',
                  isCurrent && 'bg-recor-deep text-white',
                  !isCurrent && isComplete && 'bg-slate-200 text-slate-900',
                  !isCurrent && !isComplete && 'bg-slate-100 text-slate-600',
                )}
              >
                <span aria-hidden="true">
                  {step}/{LAST_STEP}
                </span>
                <span>
                  {t(`wizard.steps.${step}.title`)}
                  {isCurrent && (
                    <span className="sr-only">
                      {' '}
                      — {t('wizard.currentStepAnnouncement')}
                    </span>
                  )}
                </span>
              </span>
              {idx < WIZARD_STEPS.length - 1 && (
                <span aria-hidden="true" className="text-slate-400">
                  →
                </span>
              )}
            </li>
          );
        })}
      </ol>
    </nav>
  );
}
