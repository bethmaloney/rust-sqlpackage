CREATE FUNCTION [dbo].[TableFunc2]
(
    @IsActive BIT = 1
)
RETURNS TABLE
AS
RETURN
(
    SELECT [Id], [Name], [Description], [Amount], [Quantity], [CreatedDate]
    FROM [dbo].[Table3]
    WHERE [IsActive] = @IsActive
);
GO
