import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { useAppStore } from '@/store/useAppStore';
import { useDimensionTypes } from '@/hooks/useDimensions';
import { Database, Package, Activity, Clock } from 'lucide-react';

export function Dashboard() {
  const currentOrg = useAppStore((state) => state.currentOrg);
  const { data: dimensionTypes, isLoading } = useDimensionTypes(currentOrg);

  const stats = [
    {
      title: 'Dimension Types',
      value: dimensionTypes?.length || 0,
      icon: Database,
      description: 'Active dimension types'
    },
    {
      title: 'Active Units',
      value: 12, // TODO: Get from API
      icon: Package,
      description: 'Deployment units'
    },
    {
      title: 'Recent Deployments',
      value: 8, // TODO: Get from API
      icon: Activity,
      description: 'Last 24 hours'
    },
    {
      title: 'Running Jobs',
      value: 3, // TODO: Get from API
      icon: Clock,
      description: 'Currently executing'
    }
  ];

  return (
    <div className="space-y-6">
      <div>
        <h1 className="text-3xl font-bold">Dashboard</h1>
        <p className="text-muted-foreground">
          Overview of your infrastructure management system
        </p>
      </div>

      {/* Stats Grid */}
      <div className="grid gap-4 md:grid-cols-2 lg:grid-cols-4">
        {stats.map((stat) => (
          <Card key={stat.title}>
            <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
              <CardTitle className="text-sm font-medium">
                {stat.title}
              </CardTitle>
              <stat.icon className="h-4 w-4 text-muted-foreground" />
            </CardHeader>
            <CardContent>
              <div className="text-2xl font-bold">
                {isLoading ? '...' : stat.value}
              </div>
              <p className="text-xs text-muted-foreground">
                {stat.description}
              </p>
            </CardContent>
          </Card>
        ))}
      </div>

      {/* Recent Activity */}
      <Card>
        <CardHeader>
          <CardTitle>Recent Activity</CardTitle>
        </CardHeader>
        <CardContent>
          <div className="space-y-4">
            <div className="flex items-center space-x-4">
              <div className="w-2 h-2 bg-green-500 rounded-full"></div>
              <div className="flex-1">
                <p className="text-sm font-medium">Deployment completed: network-vpc</p>
                <p className="text-xs text-muted-foreground">2 minutes ago</p>
              </div>
            </div>
            <div className="flex items-center space-x-4">
              <div className="w-2 h-2 bg-blue-500 rounded-full"></div>
              <div className="flex-1">
                <p className="text-sm font-medium">Unit updated: auth-service</p>
                <p className="text-xs text-muted-foreground">15 minutes ago</p>
              </div>
            </div>
            <div className="flex items-center space-x-4">
              <div className="w-2 h-2 bg-yellow-500 rounded-full"></div>
              <div className="flex-1">
                <p className="text-sm font-medium">Dimension added: stg3-euw1</p>
                <p className="text-xs text-muted-foreground">1 hour ago</p>
              </div>
            </div>
          </div>
        </CardContent>
      </Card>
    </div>
  );
} 