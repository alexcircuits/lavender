import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { usePlatform, useUi } from '../../runtime';
import { StreetLevelViewerHost } from './StreetLevelViewerHost';

vi.mock('../../runtime', () => ({
  usePlatform: vi.fn(),
  useUi: vi.fn(),
}));

describe('StreetLevelViewerHost', () => {
  let mockPlatform: {
    mapillaryStatus: ReturnType<typeof vi.fn>;
    mapillarySearch: ReturnType<typeof vi.fn>;
  };
  let mockSetSidePanel: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    mockPlatform = {
      mapillaryStatus: vi.fn().mockResolvedValue({ configured: true }),
      mapillarySearch: vi.fn().mockResolvedValue({
        data: [
          {
            id: 'photo-1',
            captured_at: 1690000000000,
            thumb_256_url: 'https://example.com/256.jpg',
            thumb_1024_url: 'https://example.com/1024.jpg',
            compass_angle: 180,
          },
          {
            id: 'photo-2',
            captured_at: 1690000050000,
            thumb_256_url: 'https://example.com/256-2.jpg',
            thumb_1024_url: 'https://example.com/1024-2.jpg',
            compass_angle: 270,
          },
        ],
      }),
    };
    (usePlatform as unknown as ReturnType<typeof vi.fn>).mockReturnValue(mockPlatform);

    mockSetSidePanel = vi.fn();
    (useUi as unknown as ReturnType<typeof vi.fn>).mockImplementation((selector) =>
      selector({ setSidePanel: mockSetSidePanel }),
    );
  });

  it('renders unconfigured prompt when client token is missing', async () => {
    mockPlatform.mapillaryStatus.mockResolvedValue({ configured: false });

    render(<StreetLevelViewerHost />);
    await waitFor(() => {
      expect(screen.getByText('Mapillary client token required')).toBeInTheDocument();
    });

    const settingsBtn = screen.getByRole('button', { name: /Open Settings/i });
    fireEvent.click(settingsBtn);
    expect(mockSetSidePanel).toHaveBeenCalledWith('settings');
  });

  it('renders photos and required CC BY-SA attribution when configured', async () => {
    render(<StreetLevelViewerHost coordinate={{ lon: 13.4, lat: 52.5 }} />);

    await waitFor(() => {
      expect(screen.getByText(/ID: photo-1/i)).toBeInTheDocument();
    });

    expect(screen.getByText(/Street-Level View/i)).toBeInTheDocument();
    expect(screen.getByText(/180°/i)).toBeInTheDocument();
    expect(screen.getByText(/Imagery ©/i)).toBeInTheDocument();
    expect(screen.getByRole('link', { name: /CC BY-SA 4.0/i })).toBeInTheDocument();
  });

  it('switches photo when thumbnail is clicked', async () => {
    render(<StreetLevelViewerHost coordinate={{ lon: 13.4, lat: 52.5 }} />);

    await waitFor(() => {
      expect(screen.getByText(/ID: photo-1/i)).toBeInTheDocument();
    });

    const thumbs = screen.getAllByRole('button').filter((b) => b.className.includes('thumbButton'));
    expect(thumbs.length).toBe(2);

    const targetThumb = thumbs[1];
    if (targetThumb) fireEvent.click(targetThumb);
    expect(screen.getByText(/ID: photo-2/i)).toBeInTheDocument();
    expect(screen.getByText(/270°/i)).toBeInTheDocument();
  });
});
