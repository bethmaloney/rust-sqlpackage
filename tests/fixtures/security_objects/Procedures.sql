CREATE PROCEDURE [dbo].[usp_GetEmployees]
AS
BEGIN
    SELECT [Id], [Name], [Email] FROM [dbo].[Employees];
END
GO
