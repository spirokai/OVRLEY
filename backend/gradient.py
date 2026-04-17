import sys

print("DEBUG: gradient.py imports starting", file=sys.stderr)
sys.stderr.flush()

import numpy as np

print("DEBUG: numpy in gradient imported", file=sys.stderr)
sys.stderr.flush()

from scipy.signal import savgol_filter

print("DEBUG: scipy.signal imported", file=sys.stderr)
sys.stderr.flush()

from scipy.stats import zscore

print("DEBUG: scipy.stats imported", file=sys.stderr)
sys.stderr.flush()

from tsmoothie.smoother import LowessSmoother

print("DEBUG: tsmoothie imported", file=sys.stderr)
sys.stderr.flush()


def gradient(point, previous_point):
    if previous_point:
        if point.elevation is None or previous_point.elevation is None:
            return None
        horizontal_distance_m = point.distance_2d(previous_point) or 0.0
        if horizontal_distance_m <= 0:
            return 0.0
        elevation_delta_m = point.elevation - previous_point.elevation
        return (elevation_delta_m / horizontal_distance_m) * 100.0


def derive_gradients(elevations, cumulative_distances_m):
    if not elevations:
        return []
    if len(elevations) == 1 or len(cumulative_distances_m) != len(elevations):
        return [0.0 for _ in elevations]

    gradients = []
    last_index = len(elevations) - 1
    for index in range(len(elevations)):
        if index == 0:
            left_index = 0
            right_index = 1
        elif index == last_index:
            left_index = last_index - 1
            right_index = last_index
        else:
            left_index = index - 1
            right_index = index + 1

        horizontal_distance_m = (
            cumulative_distances_m[right_index] - cumulative_distances_m[left_index]
        )
        if horizontal_distance_m <= 0:
            gradients.append(0.0)
            continue

        elevation_delta_m = elevations[right_index] - elevations[left_index]
        gradients.append((elevation_delta_m / horizontal_distance_m) * 100.0)

    return gradients


def handle_outliers(gradients):
    z_threshold = 2
    window_size = 7
    interpolated_gradients = gradients.copy()
    for ii in range(len(gradients) - window_size + 1):
        window = gradients[ii : ii + window_size]
        z_scores = zscore(window)
        for jj, z_score in enumerate(z_scores):
            if abs(z_score) > z_threshold:
                interpolated_value = np.mean(window)
                interpolated_gradients[ii + jj] = interpolated_value
    return interpolated_gradients


def window_smooth(gradients, window_size=5):
    smooth_gradients = []
    for ii in range(len(gradients)):
        start_idx = max(0, ii - window_size // 2)
        end_idx = min(len(gradients), ii + window_size // 2 + 1)
        smoothed_value = sum(gradients[start_idx:end_idx]) / (end_idx - start_idx)
        smooth_gradients.append(smoothed_value)
    return smooth_gradients


def lowess_smooth(gradients, smooth_fraction=0.0005, iterations=1):
    smoother = LowessSmoother(smooth_fraction=smooth_fraction, iterations=iterations)
    smoother.smooth(gradients)
    return smoother.smooth_data[0].tolist()


def savgol_smooth(gradients):
    return savgol_filter(gradients, window_length=5, polyorder=2).tolist()


def smooth_gradients(gradients):
    # first element is always None
    gradients = gradients[1:]
    gradients.insert(0, 2 * gradients[0] - gradients[1])
    gradients = handle_outliers(gradients)
    gradients = lowess_smooth(gradients)
    return gradients
