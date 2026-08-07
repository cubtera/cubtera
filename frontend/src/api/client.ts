import type { 
  DimensionData, 
  DimensionType, 
  DimensionName, 
  ApiResponse 
} from '@/types/api';

const API_BASE = import.meta.env.VITE_API_URL || 'http://localhost:8000';

class ApiError extends Error {
  constructor(
    message: string,
    public status: number,
    public response?: Response
  ) {
    super(message);
    this.name = 'ApiError';
  }
}

async function fetchApi<T>(endpoint: string): Promise<T> {
  const response = await fetch(`${API_BASE}${endpoint}`);
  
  if (!response.ok) {
    throw new ApiError(
      `API Error: ${response.statusText}`,
      response.status,
      response
    );
  }
  
  return response.json();
}

// API functions matching backend routes
export const api = {
  // Dimension Types
  getDimensionTypes: (org: string): Promise<ApiResponse<DimensionType[]>> =>
    fetchApi(`/${org}/dimTypes`),

  // Dimensions by type
  getDimensionsByType: (org: string, type: string): Promise<ApiResponse<DimensionName[]>> =>
    fetchApi(`/${org}/dims?type=${encodeURIComponent(type)}`),

  // Dimension data by type
  getDimensionsDataByType: (org: string, type: string): Promise<ApiResponse<DimensionData[]>> =>
    fetchApi(`/${org}/dimsData?type=${encodeURIComponent(type)}`),

  // Single dimension by name
  getDimensionByName: (
    org: string, 
    type: string, 
    name: string, 
    context?: string
  ): Promise<ApiResponse<DimensionData>> => {
    const params = new URLSearchParams({
      type: type,
      n: name,
    });
    if (context) {
      params.append('context', context);
    }
    return fetchApi(`/${org}/dim?${params.toString()}`);
  },

  // Dimension defaults by type
  getDimensionDefaultsByType: (org: string, type: string): Promise<ApiResponse<DimensionData>> =>
    fetchApi(`/${org}/dimDefaults?type=${encodeURIComponent(type)}`),
};

export { ApiError }; 