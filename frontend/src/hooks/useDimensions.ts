import { useQuery } from '@tanstack/react-query';
import { api } from '@/api/client';

export function useDimensionTypes(org: string) {
  return useQuery({
    queryKey: ['dimensionTypes', org],
    queryFn: () => api.getDimensionTypes(org),
    staleTime: 5 * 60 * 1000, // 5 minutes
    enabled: !!org,
  });
}

export function useDimensionsByType(org: string, type: string) {
  return useQuery({
    queryKey: ['dimensions', org, type],
    queryFn: () => api.getDimensionsByType(org, type),
    staleTime: 5 * 60 * 1000,
    enabled: !!org && !!type,
  });
}

export function useDimensionsDataByType(org: string, type: string) {
  return useQuery({
    queryKey: ['dimensionsData', org, type],
    queryFn: () => api.getDimensionsDataByType(org, type),
    staleTime: 5 * 60 * 1000,
    enabled: !!org && !!type,
  });
}

export function useDimensionByName(
  org: string, 
  type: string, 
  name: string, 
  context?: string
) {
  return useQuery({
    queryKey: ['dimension', org, type, name, context],
    queryFn: () => api.getDimensionByName(org, type, name, context),
    staleTime: 5 * 60 * 1000,
    enabled: !!org && !!type && !!name,
  });
}

export function useDimensionDefaults(org: string, type: string) {
  return useQuery({
    queryKey: ['dimensionDefaults', org, type],
    queryFn: () => api.getDimensionDefaultsByType(org, type),
    staleTime: 5 * 60 * 1000,
    enabled: !!org && !!type,
  });
} 