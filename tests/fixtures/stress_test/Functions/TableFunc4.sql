CREATE FUNCTION [dbo].[TableFunc4]
(
    @IsActive BIT = 1
)
RETURNS TABLE
AS
RETURN
(
    SELECT [Id], [Name], [Description], [Amount], [Quantity], [CreatedDate]
    FROM [dbo].[Table5]
    WHERE [IsActive] = @IsActive
);
GO
