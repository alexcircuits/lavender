import { fireEvent, render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { ShareModal } from './ShareModal';

vi.mock('../../runtime', () => ({
  useGlobe: vi.fn(() => ({
    cameraPose: vi.fn(() => ({
      lat: 37.7749,
      lon: -122.4194,
      heightM: 25000,
      headingRad: 0,
      pitchRad: -Math.PI / 4,
      rollRad: 0,
    })),
  })),
  useWorkspace: vi.fn((selector) =>
    selector({
      layers: { aircraft: { enabled: true } },
      selection: { feed: 'aircraft', id: 'AFR123' },
      imageryId: 'blue-marble',
    }),
  ),
  usePreferences: vi.fn((selector) =>
    selector({
      visualPreset: 'noir',
      hudEnabled: true,
      scopeMaskEnabled: false,
      sonarEnabled: false,
    }),
  ),
  useUi: vi.fn((selector) =>
    selector({
      shareOpen: true,
      setShareOpen: vi.fn(),
    }),
  ),
}));

describe('ShareModal', () => {
  beforeEach(() => {
    Object.defineProperty(navigator, 'clipboard', {
      value: {
        writeText: vi.fn().mockResolvedValue(undefined),
      },
      configurable: true,
    });
  });

  it('renders modal with shareable URL and summary', () => {
    render(<ShareModal open={true} onOpenChange={vi.fn()} />);

    expect(screen.getByRole('dialog', { name: /share workspace view/i })).toBeInTheDocument();
    const input = screen.getByLabelText(/shareable workspace url/i) as HTMLInputElement;
    expect(input.value).toContain('lat=37.7749');
    expect(input.value).toContain('lon=-122.4194');
    expect(input.value).toContain('style=noir');
    expect(screen.getByText(/aircraft:AFR123/)).toBeInTheDocument();
  });

  it('copies share link on button click', async () => {
    render(<ShareModal open={true} onOpenChange={vi.fn()} />);

    const copyBtn = screen.getAllByRole('button', { name: /copy/i })[0];
    if (copyBtn) {
      fireEvent.click(copyBtn);
      expect(navigator.clipboard.writeText).toHaveBeenCalledTimes(1);
    }
  });
});
