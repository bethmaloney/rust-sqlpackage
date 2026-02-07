-- Basic synonym targeting a local table
CREATE SYNONYM [dbo].[Staff] FOR [dbo].[Employees];
GO

-- Synonym targeting a different local table
CREATE SYNONYM [dbo].[Depts] FOR [dbo].[Departments];
GO

-- Cross-database synonym (3-part name)
CREATE SYNONYM [dbo].[ExternalOrders] FOR [OtherDB].[dbo].[Orders];
GO
